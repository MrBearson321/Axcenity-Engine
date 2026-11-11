# XB10k for Godot: builds the scene from ../README.md's formulas, then
# times 60 warm-up and 600 measured frames. Run with --fixed-fps 60.
extends Node3D

const SIDE := 95
const SPACING := 4.0
const GRID := 8899
const SPINNERS := 100
const DROPS := 1000
const BLINKERS := 97
const WARMUP := 60
const MEASURED := 600

var meshes := {}
var shapes := {}
var materials := {}
var check_count := 0
var check_sum := 0.0

var build_ms := 0.0
var last_usec := 0
var frame := 0
var first_frame_ms := 0.0
var samples := PackedFloat64Array()
var fixed_step := true


func _ready() -> void:
	process_priority = -1000  # time the frame before anything else runs
	var start := Time.get_ticks_usec()
	build()
	build_ms = (Time.get_ticks_usec() - start) / 1000.0
	print("XB10K_CHECK count=%d sum=%.2f" % [check_count, check_sum])
	last_usec = Time.get_ticks_usec()


func _process(delta: float) -> void:
	var now := Time.get_ticks_usec()
	var ms := (now - last_usec) / 1000.0
	last_usec = now
	frame += 1
	if frame == 1:
		first_frame_ms = ms
	elif frame > WARMUP + 1:
		# --fixed-fps 60 isn't visible to scripts, so check its effect instead.
		if absf(delta - 1.0 / 60.0) > 1e-6:
			fixed_step = false
		samples.append(ms)
		if samples.size() == MEASURED:
			report()
			get_tree().quit()


func report() -> void:
	var sorted := samples.duplicate()
	sorted.sort()
	var total := 0.0
	for s in sorted:
		total += s
	var median := (sorted[MEASURED / 2 - 1] + sorted[MEASURED / 2]) / 2.0
	var result := {
		"engine": "Godot",
		"version": Engine.get_version_info().string,
		"renderer": RenderingServer.get_current_rendering_method(),
		"frames": MEASURED,
		"median_ms": median,
		"mean_ms": total / MEASURED,
		"p95_ms": sorted[int(ceil(MEASURED * 0.95)) - 1],
		"worst_ms": sorted[MEASURED - 1],
		"fps_from_median": 1000.0 / median,
		"build_ms": build_ms,
		"first_frame_ms": first_frame_ms,
		"fixed_timestep": Engine.physics_ticks_per_second == 60 and fixed_step,
	}
	var line := JSON.stringify(result)
	print("XB10K_RESULT " + line)
	var path := OS.get_executable_path().get_base_dir().path_join("xb10k-result.json")
	var file := FileAccess.open(path, FileAccess.WRITE)
	if file == null:
		file = FileAccess.open("user://xb10k-result.json", FileAccess.WRITE)
	if file:
		file.store_string(line + "\n")


# --- Building ---------------------------------------------------------------

func material_for(color: Color, own: bool) -> StandardMaterial3D:
	if not own and materials.has(color):
		return materials[color]
	var m := StandardMaterial3D.new()
	m.albedo_color = color
	m.roughness = 0.5
	m.metallic = 0.0
	if not own:
		materials[color] = m
	return m


func wedge_mesh(size: Vector3) -> ArrayMesh:
	# Slope rises from the bottom front edge (-Z) to the top back edge (+Z).
	var hx := size.x / 2.0
	var hy := size.y / 2.0
	var hz := size.z / 2.0
	var a := Vector3(-hx, -hy, -hz)  # bottom front left
	var b := Vector3(hx, -hy, -hz)   # bottom front right
	var c := Vector3(-hx, -hy, hz)   # bottom back left
	var d := Vector3(hx, -hy, hz)    # bottom back right
	var e := Vector3(-hx, hy, hz)    # top back left
	var f := Vector3(hx, hy, hz)     # top back right
	var st := SurfaceTool.new()
	st.begin(Mesh.PRIMITIVE_TRIANGLES)
	for tri in [[a, c, d], [a, d, b], [c, e, f], [c, f, d], [a, e, c], [b, d, f], [a, b, f], [a, f, e]]:
		for v in tri:
			st.add_vertex(v)
	st.generate_normals()
	return st.commit()


func wedge_points(size: Vector3) -> PackedVector3Array:
	var h := size / 2.0
	return PackedVector3Array([
		Vector3(-h.x, -h.y, -h.z), Vector3(h.x, -h.y, -h.z),
		Vector3(-h.x, -h.y, h.z), Vector3(h.x, -h.y, h.z),
		Vector3(-h.x, h.y, h.z), Vector3(h.x, h.y, h.z)])


func shape_assets(shape: String, size: Vector3) -> Array:
	var key := "%s %s" % [shape, size]
	if meshes.has(key):
		return [meshes[key], shapes[key]]
	var mesh: Mesh
	var col: Shape3D
	match shape:
		"Block":
			var bm := BoxMesh.new()
			bm.size = size
			mesh = bm
			var bs := BoxShape3D.new()
			bs.size = size
			col = bs
		"Ball":
			var d := minf(size.x, minf(size.y, size.z))
			var sm := SphereMesh.new()
			sm.radius = d / 2.0
			sm.height = d
			mesh = sm
			var ss := SphereShape3D.new()
			ss.radius = d / 2.0
			col = ss
		"Cylinder":
			# Along X: length size.x, diameter min(size.y, size.z). Godot's
			# cylinders stand along Y, so the node turns them (see add_body).
			var dia := minf(size.y, size.z)
			var cm := CylinderMesh.new()
			cm.top_radius = dia / 2.0
			cm.bottom_radius = dia / 2.0
			cm.height = size.x
			mesh = cm
			var cs := CylinderShape3D.new()
			cs.radius = dia / 2.0
			cs.height = size.x
			col = cs
		"Wedge":
			mesh = wedge_mesh(size)
			var ws := ConvexPolygonShape3D.new()
			ws.points = wedge_points(size)
			col = ws
	meshes[key] = mesh
	shapes[key] = col
	return [mesh, col]


func add_body(body: CollisionObject3D, parent: Node, name_: String, shape: String,
		pos: Vector3, size: Vector3, material: StandardMaterial3D) -> CollisionObject3D:
	body.name = name_
	body.position = pos
	var assets := shape_assets(shape, size)
	var mi := MeshInstance3D.new()
	mi.mesh = assets[0]
	mi.material_override = material
	var cs := CollisionShape3D.new()
	cs.shape = assets[1]
	if shape == "Cylinder":
		var turn := Basis(Vector3(0, 0, 1), PI / 2.0)
		mi.basis = turn
		cs.basis = turn
	body.add_child(mi)
	body.add_child(cs)
	parent.add_child(body)
	check_count += 1
	check_sum += pos.x + pos.y + pos.z + size.x + size.y + size.z
	return body


func folder(name_: String) -> Node3D:
	var n := Node3D.new()
	n.name = name_
	add_child(n)
	return n


func build() -> void:
	var sun := DirectionalLight3D.new()
	sun.shadow_enabled = true
	add_child(sun)
	sun.look_at_from_position(Vector3.ZERO, -Vector3(0.5, 1.0, 0.25).normalized(), Vector3(0, 0, 1))

	var env := WorldEnvironment.new()
	var environment := Environment.new()
	environment.background_mode = Environment.BG_SKY
	environment.sky = Sky.new()
	environment.sky.sky_material = ProceduralSkyMaterial.new()
	environment.tonemap_mode = Environment.TONE_MAPPER_FILMIC
	env.environment = environment
	add_child(env)

	var camera := Camera3D.new()
	camera.fov = 70.0
	camera.near = 0.1
	camera.far = 2000.0
	add_child(camera)
	camera.look_at_from_position(Vector3(-260, 180, -260), Vector3.ZERO)
	camera.current = true

	add_body(StaticBody3D.new(), self, "Floor", "Block", Vector3(0, -0.5, 0),
		Vector3(440, 1, 440), material_for(Color(0.35, 0.37, 0.40), false))

	var grid := folder("Grid")
	var spinner_folder := folder("Spinners")
	var blink_targets := {}
	for k in BLINKERS:
		blink_targets[k * (GRID / BLINKERS)] = k
	var spin_script: Script = load("res://spinner.gd")
	var blink_script: Script = load("res://blinker.gd")
	var all_shapes := ["Block", "Cylinder", "Wedge"]
	for i in GRID + SPINNERS:
		@warning_ignore("integer_division")
		var r := i / SIDE
		var x := (i % SIDE) * SPACING - (SIDE - 1) * SPACING / 2.0
		var z := r * SPACING - (SIDE - 1) * SPACING / 2.0
		var h := 1.0 + float(i * 7 % 5)
		var hue := float(i % 8) / 8.0
		var color := Color(hue, 1.0 - hue, 0.5 + hue / 2.0)
		var size := Vector3(2, h, 2)
		var pos := Vector3(x, h / 2.0, z)
		if i >= GRID:
			var k := i - GRID
			var body := AnimatableBody3D.new()
			body.sync_to_physics = false
			body.set_script(spin_script)
			body.speed = 30.0 + (k % 7) * 20.0
			add_body(body, spinner_folder, "S%d" % k, "Block", pos, size, material_for(color, false))
		else:
			var own := blink_targets.has(i)
			var mat := material_for(color, own)
			var block := add_body(StaticBody3D.new(), grid, "G%d" % i, all_shapes[i % 3], pos, size, mat)
			if own:
				var k: int = blink_targets[i]
				var blinker := Timer.new()
				blinker.set_script(blink_script)
				blinker.name = "Blink%d" % k
				blinker.wait_time = 0.1 + (k % 5) * 0.1
				blinker.block = block
				blinker.material = mat
				add_child(blinker)

	var drops_folder := folder("Drops")
	var lifter := Timer.new()
	lifter.set_script(load("res://lifter.gd"))
	var drop_mat := material_for(Color(0.95, 0.6, 0.2), false)
	for j in DROPS:
		@warning_ignore("integer_division")
		var y := j / 100
		@warning_ignore("integer_division")
		var zz := (j / 10) % 10
		var pos := Vector3((j % 10) * 3.0 - 13.5, 12.0 + y * 3.0, zz * 3.0 - 13.5)
		var body := RigidBody3D.new()
		add_body(body, drops_folder, "D%d" % j, "Ball" if j % 2 == 0 else "Block",
			pos, Vector3(1.5, 1.5, 1.5), drop_mat)
		lifter.drops.append(body)
	lifter.name = "Lifter"
	add_child(lifter)
