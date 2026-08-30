CREATE TABLE chat (
  id int PRIMARY KEY GENERATED ALWAYS AS IDENTITY,

  -- The name of this chat.
  name text,

  -- The position to assign to the next message in this chat.
  next_message_position int NOT NULL DEFAULT 0
);

CREATE TABLE chat_account (
  chat_id int NOT NULL REFERENCES chat ON DELETE CASCADE,
  account_id int NOT NULL REFERENCES account ON DELETE CASCADE,
  -- An account can only be in a chat once
  UNIQUE (chat_id, account_id)
);
