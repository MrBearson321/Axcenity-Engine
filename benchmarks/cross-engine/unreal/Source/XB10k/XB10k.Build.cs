using UnrealBuildTool;

public class XB10k : ModuleRules
{
	public XB10k(ReadOnlyTargetRules Target) : base(Target)
	{
		PCHUsage = PCHUsageMode.UseExplicitOrSharedPCHs;
		PublicDependencyModuleNames.AddRange(new string[] {
			"Core", "CoreUObject", "Engine", "ProceduralMeshComponent", "Json" });
	}
}
