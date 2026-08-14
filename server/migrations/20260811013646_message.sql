CREATE TABLE IF NOT EXISTS message (
  id int PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
  account_id int REFERENCES account,
  content text
);