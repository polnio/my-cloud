CREATE TABLE `user` (
  `id` TEXT NOT NULL PRIMARY KEY,
  `password_hash` TEXT NOT NULL,
  `display_name` TEXT
);

CREATE TABLE `group` (
  `id` TEXT NOT NULL PRIMARY KEY,
  `display_name` TEXT
);

CREATE TABLE `user_group` (
  `user_id` TEXT NOT NULL,
  `group_id` TEXT NOT NULL,
  PRIMARY KEY(`user_id`, `group_id`),
  FOREIGN KEY(`user_id`) REFERENCES `user`(`id`),
  FOREIGN KEY(`group_id`) REFERENCES `group`(`id`)
);
