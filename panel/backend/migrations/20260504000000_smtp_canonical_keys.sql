-- Normalize legacy SMTP setting keys from the original settings migration.
INSERT INTO settings (key, value, updated_at)
SELECT 'smtp_username', value, updated_at
FROM settings
WHERE key = 'smtp_user'
  AND value <> ''
  AND NOT EXISTS (SELECT 1 FROM settings WHERE key = 'smtp_username' AND value <> '')
ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()
WHERE settings.value = '';

INSERT INTO settings (key, value, updated_at)
SELECT 'smtp_password', value, updated_at
FROM settings
WHERE key = 'smtp_pass'
  AND value <> ''
  AND NOT EXISTS (SELECT 1 FROM settings WHERE key = 'smtp_password' AND value <> '')
ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()
WHERE settings.value = '';

INSERT INTO settings (key, value) VALUES
    ('smtp_username', ''),
    ('smtp_password', ''),
    ('smtp_from_name', 'DockPanel'),
    ('smtp_encryption', 'starttls')
ON CONFLICT (key) DO NOTHING;
