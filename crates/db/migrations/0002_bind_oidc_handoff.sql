-- Existing in-flight logins have no browser binding and must restart.
ALTER TABLE oidc_login_requests ADD COLUMN browser_challenge TEXT;
ALTER TABLE session_handoffs ADD COLUMN browser_challenge TEXT;
