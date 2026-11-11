// XB10k for Unreal: builds the scene from ../README.md's formulas when play
// starts, then times 60 warm-up and 600 measured frames.
#pragma once

#include "CoreMinimal.h"
#include "GameFramework/GameModeBase.h"
#include "XB10kGameMode.generated.h"

class UStaticMesh;
class UMaterialInterface;
class UMaterialInstanceDynamic;
class AXBBody;

UCLASS()
class AXB10kGameMode : public AGameModeBase
{
	GENERATED_BODY()
public:
	AXB10kGameMode();
	virtual void StartPlay() override;
	virtual void EndPlay(const EEndPlayReason::Type Reason) override;

private:
	void Build();
	AXBBody* AddBody(UClass* Class, const FString& Name, const FString& Shape,
		const FVector& Pos, const FVector& Size, UMaterialInstanceDynamic* Material);
	UMaterialInstanceDynamic* MaterialFor(const FLinearColor& Color, bool bOwn);
	void OnBeginFrame();
	void Report();

	UPROPERTY() TObjectPtr<UStaticMesh> Cube;
	UPROPERTY() TObjectPtr<UStaticMesh> Sphere;
	UPROPERTY() TObjectPtr<UStaticMesh> Cylinder;
	UPROPERTY() TObjectPtr<UMaterialInterface> BaseMaterial;
	UPROPERTY() TMap<FLinearColor, TObjectPtr<UMaterialInstanceDynamic>> Materials;

	int32 CheckCount = 0;
	double CheckSum = 0.0;
	double BuildMs = 0.0;
	double FirstFrameMs = 0.0;
	double Last = 0.0;
	int32 Frame = 0;
	TArray<double> Samples;
	FDelegateHandle BeginFrameHandle;
};
