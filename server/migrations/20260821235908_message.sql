CREATE TABLE message (
  id int PRIMARY KEY GENERATED ALWAYS AS IDENTITY,

  -- The account that sent the message.
  account_id int NOT NULL REFERENCES account ON DELETE CASCADE,

  -- The chat in which the message was sent.
  chat_id int NOT NULL REFERENCES chat ON DELETE CASCADE,

  -- The textual data of the message.
  content text NOT NULL,

  -- The position of this message in its chat.
  chat_position int NOT NULL,

  UNIQUE (chat_id, chat_position)
);

CREATE FUNCTION notify_new_message()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
  PERFORM pg_notify('new_message', chat_id::text)
  FROM (SELECT DISTINCT chat_id FROM new_messages) AS chats;

  RETURN NULL;
END;
$$;

CREATE TRIGGER new_message
AFTER INSERT ON message
REFERENCING NEW TABLE AS new_messages
FOR EACH STATEMENT
EXECUTE FUNCTION notify_new_message();
