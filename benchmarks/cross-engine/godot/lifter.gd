# XB10k: lifts 100 drops back up every second, so some always fall.
extends Timer

var drops: Array[RigidBody3D] = []
var n := 1

func _ready() -> void:
	wait_time = 1.0
	timeout.connect(_on_timeout)
	start()

func _on_timeout() -> void:
	for _i in 100:
		var drop := drops[n - 1]
		var x := (n % 10) * 3.0 - 13.5
		@warning_ignore("integer_division")
		var z := ((n / 10) % 10) * 3.0 - 13.5
		var t := Transform3D(Basis.IDENTITY, Vector3(x, 40.0, z))
		PhysicsServer3D.body_set_state(drop.get_rid(), PhysicsServer3D.BODY_STATE_TRANSFORM, t)
		PhysicsServer3D.body_set_state(drop.get_rid(), PhysicsServer3D.BODY_STATE_LINEAR_VELOCITY, Vector3.ZERO)
		PhysicsServer3D.body_set_state(drop.get_rid(), PhysicsServer3D.BODY_STATE_ANGULAR_VELOCITY, Vector3.ZERO)
		n = n % drops.size() + 1
