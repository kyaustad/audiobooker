import { integer, real, sqliteTable, text } from "drizzle-orm/sqlite-core";

export const users = sqliteTable("users", {
  id: integer("id").primaryKey({ autoIncrement: true }),
  username: text("username").notNull().unique(),
  passwordHash: text("password_hash").notNull(),
  createdAt: integer("created_at", { mode: "timestamp" })
    .notNull()
    .$defaultFn(() => new Date()),
});

export const downloads = sqliteTable("downloads", {
  id: integer("id").primaryKey({ autoIncrement: true }),
  userId: integer("user_id")
    .notNull()
    .references(() => users.id, { onDelete: "cascade" }),
  magnetUri: text("magnet_uri").notNull(),
  name: text("name"),
  infoHash: text("info_hash").notNull(),
  status: text("status", {
    enum: [
      "pending",
      "downloading",
      "completed",
      "copying",
      "copied",
      "error",
    ],
  })
    .notNull()
    .default("pending"),
  progress: real("progress").notNull().default(0),
  downloadSpeed: integer("download_speed").notNull().default(0),
  eta: integer("eta").notNull().default(0),
  savePath: text("save_path"),
  contentPath: text("content_path"),
  destinationPath: text("destination_path"),
  errorMessage: text("error_message"),
  createdAt: integer("created_at", { mode: "timestamp" })
    .notNull()
    .$defaultFn(() => new Date()),
  completedAt: integer("completed_at", { mode: "timestamp" }),
  copiedAt: integer("copied_at", { mode: "timestamp" }),
});

export type User = typeof users.$inferSelect;
export type Download = typeof downloads.$inferSelect;
export type NewDownload = typeof downloads.$inferInsert;
