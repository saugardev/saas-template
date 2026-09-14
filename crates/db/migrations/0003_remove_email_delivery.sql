-- Email delivery and recovery were removed; existing users and sessions remain.
DROP TABLE email_verification_tokens;
DROP TABLE password_reset_tokens;
