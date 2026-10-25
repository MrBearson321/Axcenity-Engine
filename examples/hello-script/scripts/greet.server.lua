-- Each script has its own global variables, so this `message` can't clash
-- with a `message` in another script.
-- selene: allow(unscoped_variables)
message = "Hello from " .. script.Name .. "!"
print(message)
