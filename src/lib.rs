#![deny(missing_docs)]

//! A lightweight data cache for CLI libraries
//!
//! For CLIs that query a default set of information with each invocation,
//! `Tote` offers a convenient way to cache data to a file for quicker
//! subsequent CLI runs.
//!
//! When `Tote` is used for a cache (examples below), the `tote.get()` call will:
//! - Check that the `Tote` filepath exists and has been modified within the specified expiry time
//! - Deserialize and return the data
//!
//! If the cached data is not present or expired, `Tote` will:
//! - Use the `Fetch::fetch` or `AsyncFetch::fetch` methods to retrieve the data
//! - Serialize the data (using `serde_json`) and write to the `Tote` filepath
//! - Return the newly fetched data

//! ```ignore
//! use std::time::Duration;
//! use serde_derive::{Serialize, Deserialize};
//! use tote::{Fetch, Tote};
//!
//! // Implement `serde`'s `Serialize`/`Deserialize` for you own data
//! // or make a NewType and `derive` so `Tote` can read and write the cached data
//! #[derive(Debug, Deserialize, Serialize)]
//! struct MyData(Vec<String>);
//!
//! impl Fetch<MyData> for MyData {
//!     fn fetch() -> Result<MyData, Box<dyn std::error::Error>> {
//!         // This would likely do some I/O to fetch common data
//!         Ok(MyData(vec!["Larkspur".to_owned(), "Lavender".to_owned(), "Periwinkle".to_owned()]))
//!     }
//! }
//!
//! fn main () -> Result<(), Box<dyn std::error::Error>> {
//!     // Create a Tote at the given path, with data expiry of 1 day
//!     let cache: Tote<MyData> = Tote::new("./colors.cache", Duration::from_secs(86400));
//!
//!     // This `.get()` call will use data cached in "colors.cache" if:
//!     // - The file exists & has valid data
//!     // - The file has been modified in the past 1 day
//!     // Otherwise `MyData::fetch` is called to get the data and populate the cache file
//!     let available_colors = cache.get()?;
//!     println!("Colors you can use are: {:?}", available_colors);
//!     Ok(())
//! }
//! ```
//!
//! # Feature "async"
//! Use async/await for fetching data with the "async" feature, which presents an `AsyncFetch` trait:
//! ```toml
//! tote = { version = "*", features = ["async"] }
//! ```
//!
//! ```ignore
//! use std::collections::HashMap;
//! use std::net::IpAddr;
//! use std::time::Duration;
//!
//! use async_trait::async_trait;
//! use serde_derive::{Serialize, Deserialize};
//! use tote::{AsyncFetch, Tote};
//!
//! // Implement `serde`'s `Serialize`/`Deserialize` for you own data
//! // or make a NewType and `derive` so `Tote` can read and write the cached data
//! #[derive(Debug, Deserialize, Serialize)]
//! struct MyData(IpAddr);
//!
//! #[async_trait]
//! impl AsyncFetch<MyData> for MyData {
//!     async fn fetch() -> Result<MyData, Box<dyn std::error::Error>> {
//!        let resp = reqwest::get("https://httpbin.org/ip")
//!            .await?
//!            .json::<HashMap<String, String>>()
//!            .await?;
//!         let origin_ip = resp["origin"].parse()?;
//!         Ok(MyData(origin_ip))
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main () -> Result<(), Box<dyn std::error::Error>> {
//!     // Create a Tote at the given path, with data expiry of 1 day
//!     let cache: Tote<MyData> = Tote::new("./colors.cache", Duration::from_secs(86400));
//!     // This `.get().await` call will use data cached in "colors.cache" if:
//!     // - The file exists & has valid data
//!     // - The file has been modified in the past 1 day
//!     // Otherwise `MyData::fetch` is called to get the data and populate the cache file
//!     let available_colors = cache.get().await?;
//!     println!("Colors you can use are: {:?}", available_colors);
//!     Ok(())
//! }
//! ```
use std::fs;
use std::io::{self, Write};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(feature = "async")]
use async_trait::async_trait;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[cfg(not(feature = "async"))]
/// A trait provided to allow `Tote` to fetch the data
/// when no cache exists or cache is expired
pub trait Fetch<T> {
    #[cfg(not(feature = "async"))]
    /// Strategy for fetching data to cache
    fn fetch() -> std::result::Result<T, Box<dyn std::error::Error>>;
}

#[cfg(feature = "async")]
/// A trait provided to allow `Tote` to fetch the data
/// when no cache exists or cache is expired
#[async_trait]
pub trait AsyncFetch<T> {
    #[cfg(feature = "async")]
    /// Strategy for fetching data to cache
    async fn fetch() -> std::result::Result<T, Box<dyn std::error::Error>>;
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
    /// Cached data is missing or cannot be read
    #[error("Cached data is not valid")]
    InvalidCache,
    /// Error while fetching data
    #[error(transparent)]
    Fetching(#[from] Box<dyn std::error::Error>),
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

    #[cfg(not(feature = "async"))]
    /// Fetch the cached data, returning Err for I/O issues
    /// or if the cache file is expired
    pub fn get<'a>(&self) -> Result<T, ToteError>
    where
        for<'de> T: Deserialize<'de> + 'a,
        T: Serialize + Fetch<T>,
    {
        if let Ok(data) = self.read() {
            return Ok(data);
        }
        // Fall-back to fetching data and updating cache file
        let data = T::fetch()?;
        self.put(&data)?;
        Ok(data)
    }

    #[cfg(feature = "async")]
    /// Fetch the cached data, returning Err for I/O issues
    /// or if the cache file is expired
    pub async fn get<'a>(&self) -> Result<T, ToteError>
    where
        for<'de> T: Deserialize<'de> + 'a,
        T: Serialize + AsyncFetch<T>,
    {
        if let Ok(data) = self.read() {
            return Ok(data);
        }
        // Fall-back to fetching data and updating cache file
        let data = T::fetch().await?;
        self.put(&data)?;
        Ok(data)
    }

    fn read<'a>(&self) -> Result<T, ToteError>
    where
        for<'de> T: Deserialize<'de> + 'a,
    {
        if self.is_valid() {
            // If the cache file is valid (exists & not expired)
            // attempt to deserialize.
            // If either fails, fall through and re-fetch the data below
            let contents = fs::read_to_string(&self.path)?;
            let data = serde_json::from_str::<T>(&contents)?;
            return Ok(data);
        }
        Err(ToteError::InvalidCache)
    }

    /// Write new or updated device cache data
    fn put(&self, value: &T) -> Result<(), ToteError>
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

    #[cfg(feature = "async")]
    #[async_trait]
    impl AsyncFetch<TestData> for TestData {
        async fn fetch() -> Result<TestData, Box<dyn std::error::Error>> {
            Ok(TestData {
                name: "Test".to_owned(),
                value: 50,
            })
        }
    }

    #[cfg(not(feature = "async"))]
    impl Fetch<TestData> for TestData {
        fn fetch() -> Result<TestData, Box<dyn std::error::Error>> {
            Ok(TestData {
                name: "Test".to_owned(),
                value: 50,
            })
        }
    }

    #[cfg(not(feature = "async"))]
    #[test]
    fn test_round_trip() {
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
        let res = cache.get().unwrap();
        assert!(cache.is_valid());
        assert_eq!(res.name, "Test".to_owned());
        assert_eq!(res.value, 50);

        std::thread::sleep(Duration::from_millis(305));
        assert!(!cache.is_valid());
    }

    #[cfg(feature = "async")]
    #[tokio::test]
    async fn test_round_trip_async() {
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

    #[cfg(feature = "async")]
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
