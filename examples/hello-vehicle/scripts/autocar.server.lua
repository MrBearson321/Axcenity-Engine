-- A Script. Nobody sits in the blue AutoCar: setting its seat's Throttle
-- and Steer from Lua drives it, round and round. Walk into the red Car's
-- seat (on its left) to drive it yourself with WASD or a stick, and up the
-- ramp ahead of it. Space gets out.

local seat = workspace.AutoCar.DriverSeat

seat.Throttle = 0.5
seat.Steer = 1
print("AutoCar is driving itself")
