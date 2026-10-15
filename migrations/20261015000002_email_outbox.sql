-- Email outbox: a request only writes a row here; a worker sends it with
-- retries, so a provider outage never loses a signup or fails the page.

CREATE TABLE email_outbox (
    id              bigserial   PRIMARY KEY,
    -- What to send, e.g. 'waitlist_confirm'. The worker builds the message.
    kind            text        NOT NULL,
    -- The signup it is about; deleting the signup (leaving) drops unsent mail.
    signup_id       uuid        REFERENCES waitlist_signups (id) ON DELETE CASCADE,
    payload         jsonb       NOT NULL DEFAULT '{}'::jsonb,
    attempts        integer     NOT NULL DEFAULT 0,
    next_attempt_at timestamptz NOT NULL DEFAULT now(),
    -- A claimed row is leased until this time; a crashed worker's lease expires.
    locked_until    timestamptz,
    last_error      text,
    sent_at         timestamptz,
    -- Gave up: a permanent provider error or too many attempts.
    failed_at       timestamptz,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX email_outbox_due_idx
    ON email_outbox (next_attempt_at)
    WHERE sent_at IS NULL AND failed_at IS NULL;

CREATE INDEX email_outbox_signup_idx ON email_outbox (signup_id);
