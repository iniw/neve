create table if not exists account (
  id bigint generated always as identity primary key,
  username text not null unique,
  password text not null
);