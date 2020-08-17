# Tote

A lightweight data cache for CLI libraries

For CLIs that query a default set of information with each invocation,
`Tote` offers a convenient way to cache data to a file for quicker
subsequent CLI runs.

When `Tote` is used for a cache (examples below), the `tote.get()` call will:
- Check that the `Tote` filepath exists and has been modified within the specified expiry time
- Deserialize and return the data

If the cached data is not present or expired, `Tote` will:
- Use the `Fetch::fetch` or `AsyncFetch::fetch` methods to retrieve the data
- Serialize the data (using `serde_json`) and write to the `Tote` filepath
- Return the newly fetched data


## Features
### Default
The default feature uses a Synchronous `Fetch` trait:

```rust
use std::time::Duration;
use serde_derive::{Serialize, Deserialize};
use tote::{Fetch, Tote, Result};

// Implement `serde`'s `Serialize`/`Deserialize` for you own data
// or make a NewType and `derive` so `Tote` can read and write the cached data
#[derive(Debug, Deserialize, Serialize)]
struct MyData(Vec<String>);
impl Fetch<MyData> for MyData {
    fn fetch() -> Result<MyData> {
        // This would likely do some I/O to fetch common data
        Ok(MyData(vec!["Larkspur".to_owned(), "Lavender".to_owned(), "Periwinkle".to_owned()]))
    }
}

fn main () -> Result<()> {
    // Create a Tote at the given path, with data expiry of 1 day
    let cache: Tote<MyData> = Tote::new("./colors.cache", Duration::from_secs(86400));

    // This `.get()` call will use data cached in "colors.cache" if:
    // - The file exists & has valid data
    // - The file has been modified in the past 1 day
    // Otherwise `MyData::fetch` is called to get the data and populate the cache file
    let available_colors = cache.get()?;
    println!("Colors you can use are: {:?}", available_colors);
    Ok(())
}
```

### Feature 'async'
Specify the `"async"` feature for an Asynchronous (via Futures) for fetching data:

#### Cargo.toml
```toml
tote = { version = "*", features = ["async"] }
```

```rust
use std::time::Duration;
use async_trait::async_trait;
use serde_derive::{Serialize, Deserialize};
use tote::{Fetch, Tote, Result};

// Implement `serde`'s `Serialize`/`Deserialize` for you own data
// or make a NewType and `derive` so `Tote` can read and write the cached data
#[derive(Debug, Deserialize, Serialize)]
struct MyData(Vec<String>);

#[async_trait]
impl Fetch<MyData> for MyData {
    async fn fetch() -> Result<MyData> {
        // This would likely do some I/O to fetch common data
        Ok(MyData(vec!["Larkspur".to_owned(), "Lavender".to_owned(), "Periwinkle".to_owned()]))
    }
}

#[tokio::main]
async fn main () -> Result<()> {
    // Create a Tote at the given path, with data expiry of 1 day
    let cache: Tote<MyData> = Tote::new("./colors.cache", Duration::from_secs(86400));

    // This `.get().await` call will use data cached in "colors.cache" if:
    // - The file exists & has valid data
    // - The file has been modified in the past 1 day
    // Otherwise `MyData::fetch` is called to get the data and populate the cache file
    let available_colors = cache.get().await?;
    println!("Colors you can use are: {:?}", available_colors);
    Ok(())
}
```