pub mod client;
pub mod locator;
pub mod mapper;

use std::path::Path;
use std::sync::Arc;

pub use client::LcuClient;
pub use locator::Locator;

pub struct LcuService {
    pub locator: Arc<Locator>,
    pub client: LcuClient,
}

impl LcuService {
    pub fn new(root: &Path) -> anyhow::Result<Self> {
        let locator = Arc::new(Locator::new(root));
        let client = LcuClient::new(locator.clone())?;
        Ok(Self { locator, client })
    }
}
