// XB10k for s&box: builds the scene from ../README.md's formulas when a game
// scene starts (not in the editor), then times 60 warm-up and 600 measured
// frames. A GameObjectSystem runs in every scene, so the project needs no
// scene content: the startup scene's own objects are removed except its sky.
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Linq;
using Sandbox;

public sealed class XB10kSystem : GameObjectSystem
{
	const int Side = 95, Grid = 8899, Spinners = 100, Drops = 1000, Blinkers = 97;
	const float Spacing = 4f;
	const int Warmup = 60, Measured = 600;

	readonly Dictionary<string, Model> models = new();
	readonly Stopwatch clock = new();
	readonly List<double> samples = new( Measured );
	bool built;
	double buildMs, firstFrameMs, last;
	int frame, checkCount;
	double checkSum;

	public XB10kSystem( Scene scene ) : base( scene )
	{
		Listen( Stage.StartUpdate, -10000, OnFrame, "XB10k" );
	}

	// Spec axes are Y up; s&box's are Z up (right-handed).
	public static Vector3 ToSbox( Vector3 v ) => new( v.x, -v.z, v.y );

	/// <summary>
	/// Builds the scene again and times a new run. run-all.ps1 calls this
	/// (through sbox-cli) after turning vsync and the frame cap off, which
	/// s&box doesn't keep between launches.
	/// </summary>
	public void Restart()
	{
		built = false;
		samples.Clear();
		models.Clear();
		frame = 0;
		checkCount = 0;
		checkSum = 0;
	}

	void OnFrame()
	{
		if ( Scene.IsEditor ) return;
		if ( !built )
		{
			built = true;
			clock.Restart();
			Build();
			buildMs = clock.Elapsed.TotalMilliseconds;
			Log.Info( $"XB10K_CHECK count={checkCount} sum={checkSum:F2}" );
			last = clock.Elapsed.TotalMilliseconds;
			return;
		}
		if ( samples.Count == Measured ) return;
		double now = clock.Elapsed.TotalMilliseconds;
		double ms = now - last;
		last = now;
		frame++;
		if ( frame == 1 ) { firstFrameMs = ms; return; }
		if ( frame <= Warmup + 1 ) return;
		samples.Add( ms );
		if ( samples.Count == Measured ) Report();
	}

	void Report()
	{
		var sorted = samples.OrderBy( s => s ).ToList();
		double median = (sorted[Measured / 2 - 1] + sorted[Measured / 2]) / 2;
		string json = "{" +
			"\"engine\":\"s&box\",\"version\":\"" + Application.Version + "\",\"renderer\":\"Source 2\"," +
			$"\"frames\":{Measured},\"median_ms\":{median:F4},\"mean_ms\":{sorted.Average():F4}," +
			$"\"p95_ms\":{sorted[(int)Math.Ceiling( Measured * 0.95 ) - 1]:F4},\"worst_ms\":{sorted[Measured - 1]:F4}," +
			$"\"fps_from_median\":{1000 / median:F1},\"build_ms\":{buildMs:F1},\"first_frame_ms\":{firstFrameMs:F2}," +
			"\"fixed_timestep\":false}";
		Log.Info( "XB10K_RESULT " + json );
		FileSystem.Data.WriteAllText( "xb10k-result.json", json + "\n" );
	}

	// --- Building ---------------------------------------------------------

	static readonly Material Lit = Material.Load( "materials/default.vmat" );

	static void AddTriangle( List<Vertex> verts, Vector3 a, Vector3 b, Vector3 c )
	{
		var n = Vector3.Cross( b - a, c - a ).Normal;
		verts.Add( new Vertex( a, n, Vector3.Zero, Vector4.Zero ) );
		verts.Add( new Vertex( b, n, Vector3.Zero, Vector4.Zero ) );
		verts.Add( new Vertex( c, n, Vector3.Zero, Vector4.Zero ) );
	}

	// Each shape and size is one Model, with its own collision, shared by
	// every object of that shape and size (a few dozen in all).
	Model ModelFor( string shape, Vector3 size )
	{
		var key = $"{shape} {size}";
		if ( models.TryGetValue( key, out var cached ) ) return cached;
		var verts = new List<Vertex>();
		var builder = Model.Builder;
		var h = size / 2f;
		Vector3 P( float x, float y, float z ) => ToSbox( new Vector3( x, y, z ) );
		switch ( shape )
		{
			case "Block":
			{
				var c = new[] { P(-h.x,-h.y,-h.z), P(h.x,-h.y,-h.z), P(h.x,h.y,-h.z), P(-h.x,h.y,-h.z),
					P(-h.x,-h.y,h.z), P(h.x,-h.y,h.z), P(h.x,h.y,h.z), P(-h.x,h.y,h.z) };
				int[][] faces = { new[]{0,1,2,3}, new[]{5,4,7,6}, new[]{4,0,3,7}, new[]{1,5,6,2}, new[]{3,2,6,7}, new[]{4,5,1,0} };
				foreach ( var f in faces ) { AddTriangle( verts, c[f[0]], c[f[1]], c[f[2]] ); AddTriangle( verts, c[f[0]], c[f[2]], c[f[3]] ); }
				builder = builder.AddCollisionBox( ToSbox( h ).Abs() );
				break;
			}
			case "Ball":
			{
				float r = MathF.Min( size.x, MathF.Min( size.y, size.z ) ) / 2f;
				const int Rings = 12, Segments = 24;
				Vector3 S( int i, int j )
				{
					float t = MathF.PI * i / Rings, p = 2f * MathF.PI * j / Segments;
					return new Vector3( MathF.Sin( t ) * MathF.Cos( p ), MathF.Sin( t ) * MathF.Sin( p ), MathF.Cos( t ) ) * r;
				}
				for ( int i = 0; i < Rings; i++ )
					for ( int j = 0; j < Segments; j++ )
					{
						AddTriangle( verts, S( i, j ), S( i + 1, j ), S( i + 1, j + 1 ) );
						AddTriangle( verts, S( i, j ), S( i + 1, j + 1 ), S( i, j + 1 ) );
					}
				builder = builder.AddCollisionSphere( r );
				break;
			}
			case "Cylinder":
			{
				// Along X: length size.x, diameter min(size.y, size.z).
				float r = MathF.Min( size.y, size.z ) / 2f;
				const int Segments = 24;
				var hull = new List<Vector3>();
				for ( int j = 0; j < Segments; j++ )
				{
					float a0 = 2f * MathF.PI * j / Segments, a1 = 2f * MathF.PI * (j + 1) / Segments;
					var l0 = P( -h.x, MathF.Cos( a0 ) * r, MathF.Sin( a0 ) * r ); var r0 = P( h.x, MathF.Cos( a0 ) * r, MathF.Sin( a0 ) * r );
					var l1 = P( -h.x, MathF.Cos( a1 ) * r, MathF.Sin( a1 ) * r ); var r1 = P( h.x, MathF.Cos( a1 ) * r, MathF.Sin( a1 ) * r );
					AddTriangle( verts, l0, r0, r1 ); AddTriangle( verts, l0, r1, l1 );
					AddTriangle( verts, P( -h.x, 0, 0 ), l1, l0 ); AddTriangle( verts, P( h.x, 0, 0 ), r0, r1 );
					hull.Add( l0 ); hull.Add( r0 );
				}
				builder = builder.AddCollisionHull( hull );
				break;
			}
			case "Wedge":
			{
				// Slope rises from the bottom front edge (-Z) to the top back edge (+Z).
				var a = P(-h.x,-h.y,-h.z); var b = P(h.x,-h.y,-h.z); var c = P(-h.x,-h.y,h.z);
				var d = P(h.x,-h.y,h.z); var e = P(-h.x,h.y,h.z); var f = P(h.x,h.y,h.z);
				Vector3[][] tris = { new[]{a,c,d}, new[]{a,d,b}, new[]{c,e,f}, new[]{c,f,d}, new[]{a,e,c}, new[]{b,d,f}, new[]{a,b,f}, new[]{a,f,e} };
				foreach ( var t in tris ) AddTriangle( verts, t[0], t[1], t[2] );
				builder = builder.AddCollisionHull( new List<Vector3> { a, b, c, d, e, f } );
				break;
			}
		}
		// Both windings, so a handedness mix-up can't hide faces.
		int count = verts.Count;
		for ( int i = 0; i < count; i += 3 )
		{
			var n = -verts[i].Normal;
			verts.Add( verts[i] with { Normal = n } ); verts.Add( verts[i + 2] with { Normal = n } ); verts.Add( verts[i + 1] with { Normal = n } );
		}
		var mesh = new Mesh( Lit );
		mesh.CreateVertexBuffer( verts.Count, verts );
		mesh.Bounds = BBox.FromPoints( verts.Select( v => v.Position ) );
		var model = builder.AddMesh( mesh ).Create();
		models[key] = model;
		return model;
	}

	GameObject AddBody( GameObject parent, string name, string shape, Vector3 pos, Vector3 size, Color color )
	{
		var go = new GameObject( parent, true, name );
		go.WorldPosition = ToSbox( pos );
		var model = ModelFor( shape, size );
		var renderer = go.Components.Create<ModelRenderer>();
		renderer.Model = model;
		renderer.Tint = color;
		var collider = go.Components.Create<ModelCollider>();
		collider.Model = model;
		checkCount++;
		checkSum += pos.x + pos.y + pos.z + size.x + size.y + size.z;
		return go;
	}

	void Build()
	{
		foreach ( var child in Scene.Children.ToList() )
		{
			if ( child.Components.Get<SkyBox2D>() is null ) child.Destroy();
		}
		Scene.PhysicsWorld.Gravity = Vector3.Down * 196.2f;

		var sunGo = new GameObject( true, "Sun" );
		var sun = sunGo.Components.Create<DirectionalLight>();
		sun.Shadows = true;
		// README: sun shadows reach 700 units. s&box's sun shadow distance is
		// the r.shadows.csm.distance console variable; log what it ends up as.
		ConsoleSystem.SetValue( "r.shadows.csm.distance", 700 );
		Log.Info( $"XB10K_SHADOW_DISTANCE {ConsoleSystem.GetValue( "r.shadows.csm.distance", "?" )}" );
		sunGo.WorldRotation = Rotation.LookAt( -ToSbox( new Vector3( 0.5f, 1f, 0.25f ).Normal ) );

		var camGo = new GameObject( true, "Camera" );
		var cam = camGo.Components.Create<CameraComponent>();
		cam.FovAxis = CameraComponent.Axis.Vertical;
		cam.FieldOfView = 70f;
		cam.ZNear = 0.1f;
		cam.ZFar = 2000f;
		cam.IsMainCamera = true;
		var eye = ToSbox( new Vector3( -260f, 180f, -260f ) );
		camGo.WorldPosition = eye;
		camGo.WorldRotation = Rotation.LookAt( -eye );

		var root = new GameObject( true, "XB10k" );
		AddBody( root, "Floor", "Block", new Vector3( 0, -0.5f, 0 ), new Vector3( 440, 1, 440 ), new Color( 0.35f, 0.37f, 0.40f ) );
		var grid = new GameObject( root, true, "Grid" );
		var spinners = new GameObject( root, true, "Spinners" );
		var blinkTargets = new Dictionary<int, int>();
		for ( int k = 0; k < Blinkers; k++ ) blinkTargets[k * (Grid / Blinkers)] = k;
		string[] shapes = { "Block", "Cylinder", "Wedge" };
		for ( int i = 0; i < Grid + Spinners; i++ )
		{
			float x = (i % Side) * Spacing - (Side - 1) * Spacing / 2f;
			float z = (i / Side) * Spacing - (Side - 1) * Spacing / 2f;
			float h = 1 + (i * 7 % 5);
			float hue = (i % 8) / 8f;
			var color = new Color( hue, 1 - hue, 0.5f + hue / 2f );
			var size = new Vector3( 2, h, 2 );
			var pos = new Vector3( x, h / 2f, z );
			if ( i >= Grid )
			{
				int k = i - Grid;
				var go = AddBody( spinners, "S" + k, "Block", pos, size, color );
				go.Components.Create<XBSpinner>().Speed = 30 + (k % 7) * 20;
			}
			else
			{
				var go = AddBody( grid, "G" + i, shapes[i % 3], pos, size, color );
				if ( blinkTargets.TryGetValue( i, out int k ) )
				{
					var blinker = new GameObject( root, true, "Blink" + k ).Components.Create<XBBlinker>();
					blinker.Block = go;
					blinker.Renderer = go.Components.Get<ModelRenderer>();
					blinker.Wait = 0.1f + (k % 5) * 0.1f;
				}
			}
		}
		var drops = new GameObject( root, true, "Drops" );
		var lifter = new GameObject( root, true, "Lifter" ).Components.Create<XBLifter>();
		for ( int j = 0; j < Drops; j++ )
		{
			var pos = new Vector3( (j % 10) * 3f - 13.5f, 12f + (j / 100) * 3f, ((j / 10) % 10) * 3f - 13.5f );
			var go = AddBody( drops, "D" + j, j % 2 == 0 ? "Ball" : "Block", pos, new Vector3( 1.5f ), new Color( 0.95f, 0.6f, 0.2f ) );
			lifter.Drops.Add( go.Components.Create<Rigidbody>() );
		}
	}
}
