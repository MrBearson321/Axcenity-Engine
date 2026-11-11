using UnrealBuildTool;

public class XB10kEditorTarget : TargetRules
{
	public XB10kEditorTarget(TargetInfo Target) : base(Target)
	{
		Type = TargetType.Editor;
		DefaultBuildSettings = BuildSettingsVersion.Latest;
		IncludeOrderVersion = EngineIncludeOrderVersion.Latest;
		ExtraModuleNames.Add("XB10k");
	}
}
