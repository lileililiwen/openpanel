-- site-staging V002: staging mail-capture toggle.
-- Each slot carries a mail-capture flag - it defaults to off so
-- existing installs keep flowing site mail unchanged.

ALTER TABLE staging_slots ADD COLUMN mail_capture INTEGER NOT NULL DEFAULT 0;
