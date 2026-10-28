-- Seven joints between Attachments. Each constraint is inside its anchored
-- part, joining an Attachment there to one on the moving part.
--   DoorHinge: a door that swings between 0 and 110 degrees.
--   Servo: an arm that turns to the TargetAngle set below, and holds it.
--   Motor: a wheel spinning at its AngularVelocity.
--   Spring: a ball that bounces and settles at the FreeLength.
--   Winch: a rope that lifts the crate once its winch is on.
--   Rod, Socket: two pendulums. Elevator: a lift a servo raises.
--   Drill: a bit that slides out and spins.

local door = workspace.Door
local servo = workspace.ArmPost.Servo
local winch = workspace.Beam.Winch

-- Lift the arm 45 degrees: the servo turns it there and holds it up.
task.wait(0.5)
servo.TargetAngle = -45
print("servo turning to", servo.TargetAngle)

-- Push the door open; its hinge stops it at 110 degrees.
task.wait(0.5)
door.AssemblyAngularVelocity = Vector3.new(0, 6, 0)

-- Reel the crate up to 4 studs under the beam.
winch.WinchEnabled = true
print("winch on")

task.wait(4)
print(string.format(
	"door %d, arm %d, spring %.1f, rope %.1f",
	math.round(workspace.DoorPost.DoorHinge.CurrentAngle),
	math.round(servo.CurrentAngle),
	workspace.SpringTop.Spring.CurrentLength,
	winch.CurrentLength
))
