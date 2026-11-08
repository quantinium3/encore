CREATE TABLE accounts (
	id TEXT PRIMARY KEY,
	session_cookie TEXT NOT NULL,
	added_at_unix INTEGER NOT NULL,
	name TEXT NOT NULL DEFAULT '',
	detail TEXT,
	avatar_url TEXT
);

CREATE TABLE settings (
	category TEXT NOT NULL,
	key TEXT NOT NULL,
	value TEXT NOT NULL,
	PRIMARY KEY (category, key)
) WITHOUT ROWID;
