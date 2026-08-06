create table if not exists account (
  id bigint generated always as identity primary key,
  username text not null,
  password text not null
);

insert into account (username, password) values ('hello', '123'), ('test', 'hi');