use std::io;
use std::path::Path;

use crate::AccountId;

pub trait ProfileStore {
    fn adopt(&self, staged: &Path, id: &AccountId) -> io::Result<()>;
    fn discard(&self, staged: &Path);
    fn remove(&self, id: &AccountId) -> io::Result<()>;
}
