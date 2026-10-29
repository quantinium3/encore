use include_dir::{Dir, include_dir};
use rusqlite::Connection;
use rusqlite_migration::Migrations;

static MIGRATION_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/src/migrations");

fn migrations() -> Result<Migrations<'static>, rusqlite_migration::Error> {
    Migrations::from_directory(&MIGRATION_DIR)
}

pub fn run(conn: &mut Connection) -> Result<(), rusqlite_migration::Error> {
    migrations()?.to_latest(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_migrations_are_valid() {
        migrations().unwrap().validate().unwrap();
    }
}
