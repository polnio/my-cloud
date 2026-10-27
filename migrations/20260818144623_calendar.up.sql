CREATE TABLE `calendar` (
  `id` TEXT NOT NULL PRIMARY KEY,
  `user_id` TEXT NOT NULL,
  `display_name` TEXT,
  `color` TEXT,
  FOREIGN KEY(`user_id`) REFERENCES `user`(`id`)
);
