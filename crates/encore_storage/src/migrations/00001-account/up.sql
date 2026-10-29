CREATE TABLE IF NOT EXISTS accounts (
	id TEXT PRIMARY KEY,
	session_cookie TEXT NOT NULL,
	added_at_unix INTEGER NOT NULL
);
