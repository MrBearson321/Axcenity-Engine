// Builds the XB10k Windows player from the command line:
//   Unity.exe -batchmode -quit -projectPath . -executeMethod BuildXB10k.Build -logFile build.log
// It makes an empty scene (Bootstrap builds everything at startup), turns on
// URP if the project has no render pipeline yet, and writes Builds/XB10k.exe.
using System.IO;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.Rendering.Universal;

public static class BuildXB10k
{
    const string ScenePath = "Assets/XB10k/XB10k.unity";

    [MenuItem("XB10k/Build Windows Player")]
    public static void Build()
    {
        EnsurePipeline();
        EnsureLitMaterial();
        var scene = EditorSceneManager.NewScene(NewSceneSetup.EmptyScene, NewSceneMode.Single);
        var cam = new GameObject("Main Camera").AddComponent<Camera>();
        cam.tag = "MainCamera";
        if (GraphicsSettings.defaultRenderPipeline != null)
            cam.gameObject.AddComponent<UniversalAdditionalCameraData>();
        EditorSceneManager.SaveScene(scene, ScenePath);

        PlayerSettings.fullScreenMode = FullScreenMode.Windowed;
        PlayerSettings.defaultScreenWidth = 1920;
        PlayerSettings.defaultScreenHeight = 1080;
        PlayerSettings.resizableWindow = false;
        PlayerSettings.runInBackground = true;
        PlayerSettings.productName = "XB10k";

        var report = BuildPipeline.BuildPlayer(new BuildPlayerOptions
        {
            scenes = new[] { ScenePath },
            locationPathName = Path.Combine("Builds", "XB10k.exe"),
            target = BuildTarget.StandaloneWindows64,
            options = BuildOptions.None,
        });
        Debug.Log("XB10K_BUILD " + report.summary.result);
        if (Application.isBatchMode)
            EditorApplication.Exit(report.summary.result == UnityEditor.Build.Reporting.BuildResult.Succeeded ? 0 : 1);
    }

    // Nothing in the empty scene uses URP's Lit shader, so a player build
    // strips it and Shader.Find returns null. A material in Resources keeps
    // it in the build; Bootstrap takes its shader from there.
    static void EnsureLitMaterial()
    {
        const string path = "Assets/XB10k/Resources/XB10kLit.mat";
        if (AssetDatabase.LoadAssetAtPath<Material>(path) != null) return;
        Directory.CreateDirectory("Assets/XB10k/Resources");
        AssetDatabase.CreateAsset(new Material(Shader.Find("Universal Render Pipeline/Lit")), path);
        AssetDatabase.SaveAssets();
    }

    // A new Unity 6 project from the Universal 3D template uses URP; this
    // does the same for a project made from the command line.
    static void EnsurePipeline()
    {
        if (GraphicsSettings.defaultRenderPipeline != null) return;
        var rendererData = ScriptableObject.CreateInstance<UniversalRendererData>();
        AssetDatabase.CreateAsset(rendererData, "Assets/XB10k/XB10k_Renderer.asset");
        var asset = UniversalRenderPipelineAsset.Create(rendererData);
        AssetDatabase.CreateAsset(asset, "Assets/XB10k/XB10k_URP.asset");
        GraphicsSettings.defaultRenderPipeline = asset;
        for (int i = 0; i < QualitySettings.names.Length; i++)
        {
            QualitySettings.SetQualityLevel(i, false);
            QualitySettings.renderPipeline = asset;
        }
        AssetDatabase.SaveAssets();
    }
}
