# Sky and environment

The procedural sky and environment lighting.

- `DrawList::sky` (`Sky`) comes from the Lighting properties `SkyTopColor`, `SkyHorizonColor`, `GroundColor` and `EnvironmentIntensity`. The sky is procedural: a gradient plus a sun disc, drawn as a full-screen triangle (which also fixes the background-clear encoding issue above). There is no HDR skybox yet.
- Environment lighting is analytic from the same gradient: diffuse from the hemisphere, specular from the gradient blurred by roughness, combined with the split-sum approximation. Metals therefore reflect the sky. The flat ambient is kept only as a small floor (0.05).
