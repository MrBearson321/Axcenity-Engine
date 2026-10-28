-- The game test runner's functions for one spec file (Phase 53). The
-- engine runs this chunk in the spec's own environment, with that
-- environment and the host's functions (`ScriptRuntime::start_spec`). It
-- returns the function the spec's coroutine runs.
local env, host = ...

-- Suites are `describe` blocks; the file itself is the outermost one.
local root = { name = nil, parent = nil, before = {}, after = {}, level = nil }
local current = root
local tests = {}
local running = false

local function show(value)
	if type(value) == "string" then
		return string.format("%q", value)
	end
	return tostring(value)
end

-- Fails the test at the spec line that called the runner.
local function fail(message)
	local path, line = host.location()
	if path then
		error(path .. ":" .. line .. ": " .. message, 0)
	end
	error(message, 0)
end

local function declaring(what)
	if running then
		fail(what .. " can only be used outside a test, to declare one")
	end
end

function env.describe(name, body)
	declaring("describe")
	local suite = { name = tostring(name), parent = current, before = {}, after = {}, level = current.level }
	local outer = current
	current = suite
	body()
	current = outer
end

function env.it(name, body)
	declaring("it")
	local names = { tostring(name) }
	local suite = current
	while suite.parent do
		table.insert(names, 1, suite.name)
		suite = suite.parent
	end
	local _, line = host.location()
	table.insert(tests, { names = names, body = body, suite = current, level = current.level, line = line or 0 })
end

function env.beforeEach(hook)
	declaring("beforeEach")
	table.insert(current.before, hook)
end

function env.afterEach(hook)
	declaring("afterEach")
	table.insert(current.after, hook)
end

local function deep_equal(a, b, seen)
	if a == b then
		return true
	end
	if type(a) ~= "table" or type(b) ~= "table" then
		return false
	end
	seen = seen or {}
	if seen[a] == b then
		return true
	end
	seen[a] = b
	for key, value in a do
		if not deep_equal(value, b[key], seen) then
			return false
		end
	end
	for key in b do
		if a[key] == nil then
			return false
		end
	end
	return true
end

local function distance(a, b)
	if type(a) == "number" and type(b) == "number" then
		return math.abs(a - b)
	end
	local kind = typeof(a)
	if (kind == "Vector3" or kind == "Vector2") and typeof(b) == kind then
		return (a - b).Magnitude
	end
	return nil
end

-- `expect(value)`'s matchers. `negated` is `.never`'s: each passes when
-- its check fails. Matchers can be called with `.` or `:`.
local function expectation(actual, negated)
	local matchers = {}
	local function check(passes, message, never_message)
		if passes == negated then
			fail(negated and never_message or message)
		end
	end
	local function matcher(name, f)
		matchers[name] = function(first, ...)
			if first == matchers then
				return f(...)
			end
			return f(first, ...)
		end
	end

	matcher("toBe", function(expected)
		check(
			actual == expected,
			"expected " .. show(actual) .. " to be " .. show(expected),
			"expected " .. show(actual) .. " not to be " .. show(expected)
		)
	end)
	matcher("toEqual", function(expected)
		check(
			deep_equal(actual, expected),
			"expected " .. show(actual) .. " to equal " .. show(expected),
			"expected " .. show(actual) .. " not to equal " .. show(expected)
		)
	end)
	matcher("toBeNear", function(expected, tolerance)
		tolerance = tolerance or 0.001
		local away = distance(actual, expected)
		if away == nil then
			fail("toBeNear compares numbers, Vector3s or Vector2s, not " .. typeof(actual) .. " and " .. typeof(expected))
		end
		check(
			away <= tolerance,
			"expected " .. show(actual) .. " to be within " .. tolerance .. " of " .. show(expected) .. ", but it is " .. away .. " away",
			"expected " .. show(actual) .. " not to be within " .. tolerance .. " of " .. show(expected)
		)
	end)
	matcher("toBeTruthy", function()
		check(
			actual ~= nil and actual ~= false,
			"expected " .. show(actual) .. " to be truthy",
			"expected " .. show(actual) .. " not to be truthy"
		)
	end)
	matcher("toBeGreaterThan", function(limit)
		check(
			actual > limit,
			"expected " .. show(actual) .. " to be greater than " .. show(limit),
			"expected " .. show(actual) .. " not to be greater than " .. show(limit)
		)
	end)
	matcher("toBeLessThan", function(limit)
		check(
			actual < limit,
			"expected " .. show(actual) .. " to be less than " .. show(limit),
			"expected " .. show(actual) .. " not to be less than " .. show(limit)
		)
	end)
	matcher("toThrow", function(message)
		if type(actual) ~= "function" then
			fail("toThrow expects a function, not " .. typeof(actual))
		end
		local ok, err = pcall(actual)
		local text = tostring(err)
		local matches = not ok and (message == nil or string.find(text, message, 1, true) ~= nil)
		local wanted = if message then "to throw an error containing " .. show(message) else "to throw"
		local got = if ok then "it didn't throw" else "it threw " .. show(text)
		check(matches, "expected the function " .. wanted .. ", but " .. got, "expected the function not " .. wanted .. ", but " .. got)
	end)
	matcher("toHaveAttribute", function(name, ...)
		local value = actual:GetAttribute(name)
		if select("#", ...) == 0 then
			check(
				value ~= nil,
				"expected " .. show(actual) .. " to have the attribute " .. show(name),
				"expected " .. show(actual) .. " not to have the attribute " .. show(name)
			)
		else
			local expected = ...
			check(
				deep_equal(value, expected),
				"expected " .. show(actual) .. "'s attribute " .. show(name) .. " to be " .. show(expected) .. ", but it is " .. show(value),
				"expected " .. show(actual) .. "'s attribute " .. show(name) .. " not to be " .. show(expected)
			)
		end
	end)
	if not negated then
		matchers.never = expectation(actual, true)
	end
	return matchers
end

function env.expect(actual)
	return expectation(actual, false)
end

local Test = {}
env.Test = Test

-- The level the tests declared after this run in.
function Test.level(name)
	declaring("Test.level")
	current.level = tostring(name)
end

-- Runs the game on for `seconds` (one frame without), with physics,
-- controllers, agents and tweens. Returns the seconds that passed.
function Test.step(seconds)
	return task.wait(seconds or 0)
end

-- Holds a key, mouse button or input action for `seconds`, or for one
-- frame without.
function Test.press(input, seconds)
	host.press(tostring(input), true)
	task.wait(seconds or 0)
	host.press(tostring(input), false)
end

-- Clicks a button, as the player would, and waits for the frame it's in.
function Test.click(button)
	host.click(button)
	task.wait()
end

-- Types `text` into a TextBox and finishes with it (`FocusLost`).
function Test.type(text_box, text)
	host.type(text_box, tostring(text))
	task.wait()
end

-- The Instance at `path` (`Workspace.Character.Body`, or with `/`), from
-- `game`; nil when there isn't one.
function Test.find(path)
	local separator = if string.find(path, "/", 1, true) then "/" else "."
	local node = game
	for _, name in string.split(path, separator) do
		local child = node:FindFirstChild(name)
		if child == nil and node == game then
			local ok, service = pcall(game.GetService, game, name)
			child = if ok then service else nil
		end
		if child == nil then
			return nil
		end
		node = child
	end
	return node
end

-- Runs the game on until `condition()` gives a truthy value, and returns
-- it; fails after `timeout` seconds (5 without).
function Test.wait(condition, timeout)
	timeout = timeout or 5
	local waited = 0
	while true do
		local value = condition()
		if value then
			return value
		end
		if waited >= timeout then
			fail("timed out after " .. timeout .. " seconds waiting for the condition")
		end
		waited += task.wait()
	end
end

return function(spec, selected)
	local ok, err = pcall(spec)
	if not ok then
		host.finished(tostring(err))
		return
	end
	if selected == nil then
		for _, test in tests do
			host.found(test.names, test.line, test.level)
		end
		host.finished(nil)
		return
	end
	local test = tests[selected]
	if test == nil then
		host.finished("the test is no longer in the file; run the file again")
		return
	end
	running = true
	local suites = {}
	local suite = test.suite
	while suite do
		table.insert(suites, 1, suite)
		suite = suite.parent
	end
	-- `beforeEach` outermost first, the test, then `afterEach` innermost
	-- first. The first failure is the test's; `afterEach` always runs.
	local failure = nil
	local function attempt(f)
		local ran, problem = pcall(f)
		if not ran and failure == nil then
			failure = tostring(problem)
		end
	end
	for _, outer in suites do
		for _, hook in outer.before do
			if failure == nil then
				attempt(hook)
			end
		end
	end
	if failure == nil then
		attempt(test.body)
	end
	for index = #suites, 1, -1 do
		for _, hook in suites[index].after do
			attempt(hook)
		end
	end
	host.finished(failure)
end
