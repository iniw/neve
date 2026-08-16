CREATE TABLE account (
  id int PRIMARY KEY GENERATED ALWAYS AS IDENTITY,

  -- The username of the account.
  username text NOT NULL UNIQUE,

  -- The password of the account.
  password text NOT NULL
);
