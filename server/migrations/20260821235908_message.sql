CREATE TABLE message (
  id int PRIMARY KEY GENERATED ALWAYS AS IDENTITY,

  -- The account that sent the message.
  account_id int NOT NULL REFERENCES account ON DELETE CASCADE,

  -- The chat in which the message was sent.
  chat_id int NOT NULL REFERENCES chat ON DELETE CASCADE,

  -- The textual data of the message.
  content text NOT NULL
);

CREATE FUNCTION notify_new_message()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
  PERFORM pg_notify(
    'new_message',
    NEW.chat_id::text || ',' || NEW.id::text
  );

  RETURN NULL;
END;
$$;

CREATE TRIGGER new_message
AFTER INSERT ON message
FOR EACH ROW
EXECUTE FUNCTION notify_new_message();
