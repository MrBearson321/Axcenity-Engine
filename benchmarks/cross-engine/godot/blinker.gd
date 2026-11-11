# XB10k: wakes every `wait` seconds, reads a grid block and recolors it.
extends Timer

var block: Node3D
var material: StandardMaterial3D
var base: Color
var count := 0

func _ready() -> void:
	base = material.albedo_color
	timeout.connect(_on_timeout)
	start()

func _on_timeout() -> void:
	count += 1
	var height := block.global_position.y
	if count % 2 == 0 and height > 0.0:
		material.albedo_color = base
	else:
		material.albedo_color = Color(1.0, 0.5, 0.0)
