use std::path::PathBuf;

use super::{LoginError, Session};

#[derive(Debug)]
pub struct Login {
    pub session: Session,
    pub profile: PathBuf,
}

pub trait LoginProvider {
    fn login(&self) -> Result<Login, LoginError>;
}
