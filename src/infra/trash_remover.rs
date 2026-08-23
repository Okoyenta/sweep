use std::path::Path;

use crate::domain::models::BinItem;
use crate::domain::traits::{PathRemover, RecycleBin};

pub struct TrashRemover;

impl TrashRemover {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TrashRemover {
    fn default() -> Self {
        Self::new()
    }
}

impl PathRemover for TrashRemover {
    fn remove_path(&self, path: &Path) -> anyhow::Result<()> {
        trash::delete(path).map_err(|e| anyhow::anyhow!("trash delete failed: {e}"))
    }
}

pub struct TrashBin;

impl TrashBin {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TrashBin {
    fn default() -> Self {
        Self::new()
    }
}

impl RecycleBin for TrashBin {
    fn items(&self) -> anyhow::Result<Vec<BinItem>> {
        let items = trash::os_limited::list()
            .map_err(|e| anyhow::anyhow!("listing recycle bin failed: {e}"))?;
        Ok(items
            .into_iter()
            .map(|i| BinItem {
                name: i.name.to_string_lossy().into_owned(),
                original_parent: i.original_parent.to_string_lossy().into_owned(),
                deleted_unix: i.time_deleted,
            })
            .collect())
    }

    fn purge_all(&self) -> anyhow::Result<u64> {
        let items = trash::os_limited::list()
            .map_err(|e| anyhow::anyhow!("listing recycle bin failed: {e}"))?;
        let count = items.len() as u64;
        trash::os_limited::purge_all(items)
            .map_err(|e| anyhow::anyhow!("emptying recycle bin failed: {e}"))?;
        Ok(count)
    }
}
