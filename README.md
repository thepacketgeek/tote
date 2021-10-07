# Tote

A lightweight data file cache for CLI libraries

For CLIs that query a default set of information with each invocation,
`Tote` offers a convenient way to cache data to a file for quicker
subsequent CLI runs.

When `Tote` is used for a cache (examples below), the `tote.get()` call will:
- Check that the `Tote` filepath exists and has been modified within the specified expiry time
- Deserialize and return the data

If the cached data is not present or expired, `Tote` will:
- Use the `Fetch::fetch` methods to retrieve the data
- Serialize the data (using `serde_json`) and write to the `Tote` filepath
- Return the newly fetched data

## Features
### Default
The default feature uses a Synchronous `Fetch` trait:

```rust
use std::time::Duration;

use serde_derive::{Serialize, Deserialize};
use tote::{Fetch, Tote};

// Implement `serde`'s `Serialize`/`Deserialize` for you own data
// or make a NewType and `derive` so `Tote` can read and write the cached data
#[derive(Debug, Deserialize, Serialize)]
struct NearbyCities(Vec<String>);

impl Fetch for NearbyCities {
    type Cached = NearbyCities;

    fn fetch() -> Result<NearbyCities, Box<dyn std::error::Error>> {
        let resp = reqwest::blocking::get("http://getnearbycities.geobytes.com/GetNearbyCities?radius=10")?
           .json::<Vec<Vec<String>>>()?;
        let cities = resp.into_iter().map(|city| format!("{}, {}", city[1], city[2])).collect();
        Ok(NearbyCities(cities))
    }
}

fn main () -> Result<(), Box<dyn std::error::Error>> {
    // Create a Tote at the given path, with data expiry of 1 day
    let cache: Tote<NearbyCities> = Tote::new(".my_tool.cache", Duration::from_secs(86400));

    // This `.get()` call will use data cached in ".my_tool.cache" if:
    // - The file exists & has valid data
    // - The file has been modified in the past 1 day
    // Otherwise `NearbyCities::fetch` is called to get the data and populate the cache file
    let nearby_cities = cache.get()?;
    println!("Cities near you are: {:?}", nearby_cities);
    # std::fs::remove_file(".my_tool.cache")?;
    Ok(())
}
```

### Async
The `"async"` feature adds the `AsyncFetch` trait if you want to use async I/O for fetching data. Call `Tote::get_async().await` to get the `Tote` contents.

#### Cargo.toml
```toml
tote = { version = "*", features = ["async"] }
```

```rust
use std::collections::HashMap;
use std::net::IpAddr;
use std::time::Duration;

use async_trait::async_trait;
use serde_derive::{Serialize, Deserialize};
use tote::{AsyncFetch, Tote};

// Implement `serde`'s `Serialize`/`Deserialize` for you own data
// or make a NewType and `derive` so `Tote` can read and write the cached data
#[derive(Debug, Deserialize, Serialize)]
struct MyPublicIp(IpAddr);

#[async_trait]
impl AsyncFetch for MyPublicIp {
    type Cached = MyPublicIp;

    async fn fetch_async() -> Result<MyPublicIp, Box<dyn std::error::Error>> {
       let resp = reqwest::get("https://httpbin.org/ip")
           .await?
           .json::<HashMap<String, String>>()
           .await?;
        let origin_ip = resp["origin"].parse()?;
        Ok(MyPublicIp(origin_ip))
    }
}

#[tokio::main]
async fn main () -> Result<(), Box<dyn std::error::Error>> {
    // Create a Tote at the given path, with data expiry of 1 day
    let cache: Tote<MyPublicIp> = Tote::new(".my_tool.cache", Duration::from_secs(86400));
    // This `.get_async().await` call will use data cached in ".my_tool.cache" if:
    // - The file exists & has valid data
    // - The file has been modified in the past 1 day
    // Otherwise `MyPublicIp::fetch_async` is called to get the data and populate the cache file
    let public_ip = cache.get_async().await?;
    println!("Your public IP address is {}", public_ip.0);
    # std::fs::remove_file(".my_tool.cache")?;
    Ok(())
}
```