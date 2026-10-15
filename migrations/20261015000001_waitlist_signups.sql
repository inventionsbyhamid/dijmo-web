-- The waitlist, with double opt-in.
-- Emails are stored lower-cased in a citext column. Tokens are never stored:
-- only the SHA-256 of the current confirmation token. The IP is never stored:
-- only a coarse HMAC of it (IPv4 /24, IPv6 /48).

CREATE EXTENSION IF NOT EXISTS citext;

CREATE TABLE waitlist_signups (
    id               uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    email            citext      NOT NULL UNIQUE,
    -- SHA-256 of the confirmation token in the latest email, set when that
    -- email is sent (the raw token only ever exists in the email itself).
    token_hash       bytea       UNIQUE,
    token_created_at timestamptz,
    confirmed_at     timestamptz,
    -- When a confirmation email was last queued, for the re-send cooldown.
    last_enqueued_at timestamptz NOT NULL DEFAULT now(),
    -- UTM parameters present at signup, e.g. {"utm_source": "x"}.
    source           jsonb       NOT NULL DEFAULT '{}'::jsonb,
    ip_hash          bytea,
    created_at       timestamptz NOT NULL DEFAULT now(),
    updated_at       timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT waitlist_signups_email_lowercase CHECK (email::text = lower(email::text)),
    CONSTRAINT waitlist_signups_token_hash_len CHECK (token_hash IS NULL OR octet_length(token_hash) = 32)
);

-- Unconfirmed signups are pruned by the age of their last request.
CREATE INDEX waitlist_signups_unconfirmed_idx
    ON waitlist_signups (last_enqueued_at)
    WHERE confirmed_at IS NULL;
