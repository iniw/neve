CREATE TABLE chat (
  id int PRIMARY KEY GENERATED ALWAYS AS IDENTITY,

  -- The name of this chat.
  name text
);

CREATE TABLE chat_account (
  chat_id int NOT NULL REFERENCES chat ON DELETE CASCADE,
  account_id int NOT NULL REFERENCES account ON DELETE CASCADE
);
