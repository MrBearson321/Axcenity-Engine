// XB10k for Unity: builds the scene from ../README.md's formulas at startup,
// then times 60 warm-up and 600 measured frames with fixed 1/60 s game time.
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using UnityEngine;
using UnityEngine.Rendering;

[DefaultExecutionOrder(-10000)] // time the frame before any other Update
public class Bootstrap : MonoBehaviour
{
    const int Side = 95, Grid = 8899, Spinners = 100, Drops = 1000, Blinkers = 97;
    const float Spacing = 4f;
    const int Warmup = 60, Measured = 600;

    readonly Dictionary<string, Mesh> meshes = new Dictionary<string, Mesh>();
    readonly Dictionary<Color, Material> materials = new Dictionary<Color, Material>();
    Shader shader;
    int checkCount;
    double checkSum;

    readonly Stopwatch clock = new Stopwatch();
    readonly List<double> samples = new List<double>(Measured);
    double buildMs, firstFrameMs, last;
    int frame;

    [RuntimeInitializeOnLoadMethod(RuntimeInitializeLoadType.AfterSceneLoad)]
    static void Launch()
    {
        new GameObject("XB10k").AddComponent<Bootstrap>();
    }

    void Awake()
    {
        QualitySettings.vSyncCount = 0;
        Application.targetFrameRate = -1;
        Screen.SetResolution(1920, 1080, FullScreenMode.Windowed);
        Time.captureFramerate = 60;          // every frame is 1/60 s of game time
        Time.fixedDeltaTime = 1f / 60f;
        Time.maximumDeltaTime = 10f / 60f;
        Physics.gravity = new Vector3(0f, -196.2f, 0f);

        clock.Start();
        Build();
        buildMs = clock.Elapsed.TotalMilliseconds;
        UnityEngine.Debug.Log($"XB10K_CHECK count={checkCount} sum={checkSum:F2}");
        last = clock.Elapsed.TotalMilliseconds;
    }

    void Update()
    {
        double now = clock.Elapsed.TotalMilliseconds;
        double ms = now - last;
        last = now;
        frame++;
        if (frame == 1) { firstFrameMs = ms; return; }
        if (frame <= Warmup + 1) return;
        samples.Add(ms);
        if (samples.Count == Measured) { Report(); Application.Quit(); }
    }

    void Report()
    {
        samples.Sort();
        double total = 0;
        foreach (var s in samples) total += s;
        double median = (samples[Measured / 2 - 1] + samples[Measured / 2]) / 2;
        var pipeline = GraphicsSettings.currentRenderPipeline;
        string json = "{" +
            $"\"engine\":\"Unity\",\"version\":\"{Application.unityVersion}\"," +
            $"\"renderer\":\"{(pipeline ? pipeline.GetType().Name : "Built-in")} {SystemInfo.graphicsDeviceType}\"," +
            $"\"frames\":{Measured},\"median_ms\":{median:F4},\"mean_ms\":{total / Measured:F4}," +
            $"\"p95_ms\":{samples[(int)System.Math.Ceiling(Measured * 0.95) - 1]:F4},\"worst_ms\":{samples[Measured - 1]:F4}," +
            $"\"fps_from_median\":{1000 / median:F1},\"build_ms\":{buildMs:F1},\"first_frame_ms\":{firstFrameMs:F2}," +
            $"\"fixed_timestep\":true,\"scripting_backend\":\"{(System.Type.GetType("Mono.Runtime") != null ? "Mono" : "IL2CPP")}\"" +
            "}";
        UnityEngine.Debug.Log("XB10K_RESULT " + json);
        try { File.WriteAllText(Path.Combine(Path.GetDirectoryName(Application.dataPath), "xb10k-result.json"), json + "\n"); }
        catch (IOException) { }
    }

    // --- Building -------------------------------------------------------

    Material MaterialFor(Color color, bool own)
    {
        if (!own && materials.TryGetValue(color, out var shared)) return shared;
        var m = new Material(shader) { color = color };
        if (m.HasProperty("_Smoothness")) m.SetFloat("_Smoothness", 0.5f);
        if (m.HasProperty("_Glossiness")) m.SetFloat("_Glossiness", 0.5f);
        if (m.HasProperty("_Metallic")) m.SetFloat("_Metallic", 0f);
        if (!own) materials[color] = m;
        return m;
    }

    static Mesh Primitive(PrimitiveType type)
    {
        var go = GameObject.CreatePrimitive(type);
        var mesh = go.GetComponent<MeshFilter>().sharedMesh;
        Destroy(go);
        return mesh;
    }

    Mesh WedgeMesh()
    {
        // Unit wedge, scaled by the object's size: the slope rises from the
        // bottom front edge (-Z) to the top back edge (+Z).
        if (meshes.TryGetValue("Wedge", out var cached)) return cached;
        var a = new Vector3(-.5f, -.5f, -.5f); var b = new Vector3(.5f, -.5f, -.5f);
        var c = new Vector3(-.5f, -.5f, .5f); var d = new Vector3(.5f, -.5f, .5f);
        var e = new Vector3(-.5f, .5f, .5f); var f = new Vector3(.5f, .5f, .5f);
        Vector3[][] tris = {
            new[] { a, d, c }, new[] { a, b, d }, new[] { c, f, e }, new[] { c, d, f },
            new[] { a, c, e }, new[] { b, f, d }, new[] { a, f, b }, new[] { a, e, f } };
        var verts = new List<Vector3>();
        var idx = new List<int>();
        foreach (var t in tris) foreach (var v in t) { idx.Add(verts.Count); verts.Add(v); }
        var mesh = new Mesh { name = "Wedge" };
        mesh.SetVertices(verts);
        mesh.SetTriangles(idx, 0);
        mesh.RecalculateNormals();
        mesh.RecalculateBounds();
        meshes["Wedge"] = mesh;
        return mesh;
    }

    GameObject AddBody(Transform parent, string name, string shape, Vector3 pos, Vector3 size, Material material)
    {
        var go = new GameObject(name);
        go.transform.SetParent(parent, false);
        go.transform.position = pos;
        // The visual and collider sit on a child so a cylinder can lie along
        // X and a ball can take the smallest axis, as in Roblox.
        var shapeGo = new GameObject("Shape");
        shapeGo.transform.SetParent(go.transform, false);
        var filter = shapeGo.AddComponent<MeshFilter>();
        var renderer = shapeGo.AddComponent<MeshRenderer>();
        renderer.sharedMaterial = material;
        switch (shape)
        {
            case "Block":
                filter.sharedMesh = meshes["Cube"];
                shapeGo.transform.localScale = size;
                shapeGo.AddComponent<BoxCollider>();
                break;
            case "Ball":
                float dia = Mathf.Min(size.x, Mathf.Min(size.y, size.z));
                filter.sharedMesh = meshes["Sphere"];
                shapeGo.transform.localScale = Vector3.one * dia;
                shapeGo.AddComponent<SphereCollider>();
                break;
            case "Cylinder":
                float cd = Mathf.Min(size.y, size.z);
                filter.sharedMesh = meshes["Cylinder"]; // Unity's: 1 wide, 2 tall along Y
                shapeGo.transform.localRotation = Quaternion.Euler(0f, 0f, 90f);
                shapeGo.transform.localScale = new Vector3(cd, size.x / 2f, cd);
                shapeGo.AddComponent<MeshCollider>().convex = true;
                break;
            case "Wedge":
                filter.sharedMesh = WedgeMesh();
                shapeGo.transform.localScale = size;
                shapeGo.AddComponent<MeshCollider>().convex = true;
                break;
        }
        checkCount++;
        checkSum += pos.x + pos.y + pos.z + size.x + size.y + size.z;
        return go;
    }

    void Build()
    {
        shader = Shader.Find("Universal Render Pipeline/Lit") ?? Shader.Find("Standard");
        meshes["Cube"] = Primitive(PrimitiveType.Cube);
        meshes["Sphere"] = Primitive(PrimitiveType.Sphere);
        meshes["Cylinder"] = Primitive(PrimitiveType.Cylinder);

        var sun = new GameObject("Sun").AddComponent<Light>();
        sun.type = LightType.Directional;
        sun.shadows = LightShadows.Soft;
        sun.transform.rotation = Quaternion.LookRotation(-new Vector3(0.5f, 1f, 0.25f).normalized);

        var cam = Camera.main;
        if (cam == null) cam = new GameObject("Camera").AddComponent<Camera>();
        cam.fieldOfView = 70f;
        cam.nearClipPlane = 0.1f;
        cam.farClipPlane = 2000f;
        cam.transform.position = new Vector3(-260f, 180f, -260f);
        cam.transform.LookAt(Vector3.zero);

        var root = transform;
        AddBody(root, "Floor", "Block", new Vector3(0, -0.5f, 0), new Vector3(440, 1, 440),
            MaterialFor(new Color(0.35f, 0.37f, 0.40f), false));

        var grid = new GameObject("Grid").transform;
        var spinners = new GameObject("Spinners").transform;
        var blinkTargets = new Dictionary<int, int>();
        for (int k = 0; k < Blinkers; k++) blinkTargets[k * (Grid / Blinkers)] = k;
        string[] shapes = { "Block", "Cylinder", "Wedge" };
        for (int i = 0; i < Grid + Spinners; i++)
        {
            int c = i % Side, r = i / Side;
            float x = c * Spacing - (Side - 1) * Spacing / 2f;
            float z = r * Spacing - (Side - 1) * Spacing / 2f;
            float h = 1 + (i * 7 % 5);
            float hue = (i % 8) / 8f;
            var color = new Color(hue, 1 - hue, 0.5f + hue / 2f);
            var size = new Vector3(2, h, 2);
            var pos = new Vector3(x, h / 2f, z);
            if (i >= Grid)
            {
                int k = i - Grid;
                var go = AddBody(spinners, "S" + k, "Block", pos, size, MaterialFor(color, false));
                var rb = go.AddComponent<Rigidbody>();
                rb.isKinematic = true;
                go.AddComponent<Spinner>().speed = 30 + (k % 7) * 20;
            }
            else
            {
                bool own = blinkTargets.TryGetValue(i, out int k);
                var mat = MaterialFor(color, own);
                var go = AddBody(grid, "G" + i, shapes[i % 3], pos, size, mat);
                if (own)
                {
                    var blinker = new GameObject("Blink" + k).AddComponent<Blinker>();
                    blinker.block = go.transform;
                    blinker.material = mat;
                    blinker.wait = 0.1f + (k % 5) * 0.1f;
                }
            }
        }

        var dropsFolder = new GameObject("Drops").transform;
        var lifter = new GameObject("Lifter").AddComponent<Lifter>();
        var dropMat = MaterialFor(new Color(0.95f, 0.6f, 0.2f), false);
        for (int j = 0; j < Drops; j++)
        {
            int x = j % 10, y = j / 100, z = (j / 10) % 10;
            var pos = new Vector3(x * 3f - 13.5f, 12f + y * 3f, z * 3f - 13.5f);
            var go = AddBody(dropsFolder, "D" + j, j % 2 == 0 ? "Ball" : "Block", pos, Vector3.one * 1.5f, dropMat);
            lifter.drops.Add(go.AddComponent<Rigidbody>());
        }
    }
}
