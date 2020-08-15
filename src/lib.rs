#![deny(missing_docs)]

//! A lightweight data cache for CLI libraries
//!
//! For CLIs that query a default set of information with each invocation,
//! `Tote` offers a convenient way to cache data to a file for quicker
//! subsequent CLI runs.
//!
//! ```rust
//! use std::time::Duration;
//! use async_trait::async_trait;
//! use serde_derive::{Serialize, Deserialize};
//! use tote::{Fetch, Tote, Result};
//!
//! // Implement `serde`'s `Serialize`/`Deserialize` for you own data
//! // or make a NewType and `derive` so `Tote` can read and write the cached data
//! #[derive(Debug, Deserialize, Serialize)]
//! struct MyData(Vec<String>);
//!
//! #[async_trait]
//! impl Fetch<MyData> for MyData {
//!     async fn fetch() -> Result<MyData> {
//!         // This would likely do some I/O to fetch common data
//!         Ok(MyData(vec!["Larkspur".to_owned(), "Lavender".to_owned(), "Periwinkle".to_owned()]))
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main () -> Result<()> {
//!     // Create a Tote at the given path, with data expiry of 1 day
//!     let cache: Tote<MyData> = Tote::new("./colors.cache", Duration::from_secs(86400));
//!
//!     // This `.get().await` call will use data cached in "colors.cache" if:
//!     // - The file exists & has valid data
//!     // - The file has been modified in the past 1 day
//!     // Otherwise `MyData::fetch` is called to get the data and populate the cache file
//!     let available_colors = cache.get().await?;
//!     println!("Colors you can use are: {:?}", available_colors);
//!     Ok(())
//! }

use std::fs;
use std::io::{self, Write};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Tote Result alias for cache methods
pub type Result<T> = std::result::Result<T, ToteError>;

/// A trait provided to allow `Tote` to fetch the data
/// when no cache exists or cache is expired
#[async_trait]
pub trait Fetch<T> {
    /// Strategy for fetching data to cache
    async fn fetch() -> Result<T>;
}

/// Errors that can occur during `Tote` operations
#[derive(Error, Debug)]
pub enum ToteError {
    /// Error reading/writing from given cache file path
    #[error(transparent)]
    FileAccess(#[from] std::io::Error),
    /// Error with Serde (de)serialization
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}

/// Local file cache for D42 device info
///
/// Given a path & maximum cache age, provides methods
/// for fetching (unexpired) and writing device info
#[derive(Debug)]
pub struct Tote<T> {
    /// Filepath to write cached data
    path: PathBuf,
    /// Cached data older than this age is considered expired
    max_age: Duration,
    _phantom: PhantomData<T>,
}

impl<T> Tote<T> {
    /// Create a new cache for a given filepath & expiry age
    pub fn new<P: AsRef<Path>>(path: P, max_age: Duration) -> Self {
        Self {
            path: path.as_ref().to_owned(),
            max_age,
            _phantom: PhantomData,
        }
    }

    /// Fetch the cached data, returning Err for I/O issues
    /// or if the cache file is expired
    pub async fn get<'a>(&self) -> Result<T>
    where
        for<'de> T: Deserialize<'de> + 'a,
        T: Serialize + Fetch<T>,
    {
        if self.is_valid() {
            // If the cache file is valid (exists & not expired)
            // attempt to deserialize.
            // If either fails, fall through and re-fetch the data below
            if let Ok(contents) = fs::read_to_string(&self.path) {
                if let Ok(data) = serde_json::from_str::<T>(&contents) {
                    return Ok(data);
                }
            }
        }
        // Fall-back to fetching data and updating cache file
        let data = T::fetch().await?;
        self.put(&data)?;
        Ok(data)
    }

    /// Write new or updated device cache data
    fn put(&self, value: &T) -> Result<()>
    where
        T: Serialize,
    {
        let data = serde_json::to_string(value)?;
        let file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .open(&self.path)?;
        let mut writer = io::BufWriter::new(file);
        writer.write_all(&data.as_bytes())?;
        Ok(())
    }

    /// Is the cached data valid (exists & not expired)
    fn is_valid(&self) -> bool {
        fs::metadata(&self.path)
            .map_err(|_| ())
            .and_then(|metadata| metadata.modified().map_err(|_| ()))
            .and_then(|modified| modified.elapsed().map_err(|_| ()))
            .map(|age| age <= self.max_age)
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_derive::{Deserialize, Serialize};
    use tempfile::NamedTempFile;

    #[derive(Debug, Serialize, Deserialize)]
    struct TestData {
        name: String,
        value: u8,
    }

    #[async_trait]
    impl Fetch<TestData> for TestData {
        async fn fetch() -> Result<TestData> {
            Ok(TestData {
                name: "Test".to_owned(),
                value: 50,
            })
        }
    }

    #[tokio::test]
    async fn test_round_trip() {
        let file = NamedTempFile::new().unwrap();
        let cache: Tote<TestData> = Tote::new(file.path(), Duration::from_millis(300));

        // Stage cached data
        cache
            .put(&TestData {
                name: "Test".to_owned(),
                value: 50,
            })
            .unwrap();

        assert!(cache.is_valid());
        let res = cache.get().await.unwrap();
        assert!(cache.is_valid());
        assert_eq!(res.name, "Test".to_owned());
        assert_eq!(res.value, 50);

        std::thread::sleep(Duration::from_millis(305));
        assert!(!cache.is_valid());
    }

    #[tokio::test]
    async fn test_empty_file() {
        let file = NamedTempFile::new().unwrap();
        let cache: Tote<TestData> = Tote::new(file.path(), Duration::from_millis(300));

        let res = cache.get().await.unwrap();
        assert!(cache.is_valid());
        assert_eq!(res.name, "Test".to_owned());
        assert_eq!(res.value, 50);

        std::thread::sleep(Duration::from_millis(305));
        assert!(!cache.is_valid());
    }
}
