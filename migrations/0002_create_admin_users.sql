-- Create admin_users table for dashboard authentication

CREATE TABLE IF NOT EXISTS admin_users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Create an initial admin user if one doesn't exist
INSERT OR IGNORE INTO admin_users (username, password_hash)
VALUES ('admin', '$argon2id$v=19$m=65536,t=3,p=4$example_salt$example_hash');

-- Create indexes for better performance
CREATE INDEX IF NOT EXISTS idx_admin_users_username ON admin_users(username);