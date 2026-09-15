ALTER TABLE audit_events
ADD CONSTRAINT audit_events_event_type_not_empty CHECK (btrim(event_type) <> '');
