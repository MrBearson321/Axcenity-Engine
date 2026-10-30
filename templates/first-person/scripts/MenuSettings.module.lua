-- START HERE to change the pause and settings menus: the actions players
-- can rebind (names from File > Input...), and the save slot that Save and
-- Load use. The menus themselves are the GameMenu ScreenGui (open it in
-- the UI Designer; it's a prefab, assets/prefabs/GameMenu.prefab.json) and
-- menu.css.
return {
	Actions = { "Jump", "Run", "Crouch", "Primary", "Reload" },
	Slot = 1,
}
