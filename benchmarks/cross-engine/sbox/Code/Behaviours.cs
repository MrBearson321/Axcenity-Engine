// XB10k behaviours: one component per spinner and blinker, and the lifter.
using System;
using System.Collections.Generic;
using Sandbox;

public sealed class XBSpinner : Component
{
	public float Speed { get; set; } = 30f;
	float angle;

	protected override void OnUpdate()
	{
		angle = (angle + Speed * Time.Delta) % 360f;
		WorldRotation = Rotation.FromYaw( angle );
	}
}

public sealed class XBBlinker : Component
{
	public GameObject Block { get; set; }
	public ModelRenderer Renderer { get; set; }
	public float Wait { get; set; } = 0.1f;
	Color baseColor;
	int count;
	TimeUntil next;

	protected override void OnStart()
	{
		baseColor = Renderer.Tint;
		next = Wait;
	}

	protected override void OnUpdate()
	{
		if ( !next ) return;
		next = Wait;
		count++;
		// s&box is Z up.
		float height = Block.WorldPosition.z;
		Renderer.Tint = (count % 2 == 0 && height > 0f) ? baseColor : new Color( 1f, 0.5f, 0f );
	}
}

public sealed class XBLifter : Component
{
	public List<Rigidbody> Drops { get; } = new();
	int n = 1;
	TimeUntil next;

	protected override void OnStart() => next = 1f;

	protected override void OnUpdate()
	{
		if ( !next ) return;
		next = 1f;
		for ( int i = 0; i < 100; i++ )
		{
			var drop = Drops[n - 1];
			float x = (n % 10) * 3f - 13.5f;
			float z = ((n / 10) % 10) * 3f - 13.5f;
			drop.WorldPosition = XB10kSystem.ToSbox( new Vector3( x, 40f, z ) );
			drop.Velocity = Vector3.Zero;
			drop.AngularVelocity = Vector3.Zero;
			n = n % Drops.Count + 1;
		}
	}
}
