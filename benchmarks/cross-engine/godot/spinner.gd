# XB10k: turns one spinner every frame.
extends AnimatableBody3D

var speed := 30.0
var angle := 0.0

func _process(delta: float) -> void:
	angle = fmod(angle + speed * delta, 360.0)
	rotation_degrees = Vector3(0.0, angle, 0.0)
