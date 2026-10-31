-- A colour theme per connection (RC-100-51): the theme its panel and its SSH terminal draw with in
-- the light and in the dark mode, a theme's id (a built-in one's or an own one's, `themes.id`);
-- NULL draws with the window's theme of that mode.
--
-- No foreign key: a built-in theme is code, never a row of `themes`. The repository validates
-- every write, the engine that the id names a theme of the mode; a deleted own theme reads back
-- as NULL (`rc_db::connections`), so its connections fall back to the window's theme.

ALTER TABLE connections ADD COLUMN theme_light TEXT;
ALTER TABLE connections ADD COLUMN theme_dark TEXT;
