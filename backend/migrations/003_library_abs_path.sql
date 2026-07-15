-- ABS reports host/container paths that may differ from Audiobooker's mounts.
-- Keep abs_path as the ABS hint; libraries.path is always the writable
-- container root configured in Audiobooker.
ALTER TABLE libraries ADD COLUMN abs_path TEXT;
