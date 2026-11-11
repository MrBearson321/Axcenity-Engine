// XB10k actors: one per object, as an Unreal user would place them.
#pragma once

#include "CoreMinimal.h"
#include "GameFramework/Actor.h"
#include "XB10kActors.generated.h"

class UMaterialInstanceDynamic;
class UPrimitiveComponent;

/** A scene object: its root is a static mesh or (wedges) a procedural mesh. */
UCLASS()
class AXBBody : public AActor
{
	GENERATED_BODY()
public:
	AXBBody();
	UPROPERTY() TObjectPtr<UPrimitiveComponent> Shape;
};

/** Turns about the up axis every frame. */
UCLASS()
class AXBSpinner : public AXBBody
{
	GENERATED_BODY()
public:
	AXBSpinner();
	virtual void Tick(float DeltaSeconds) override;
	float Speed = 30.f;
	float Angle = 0.f;
};

/** Wakes on a looping timer, reads a grid block and recolors it. */
UCLASS()
class AXBBlinker : public AActor
{
	GENERATED_BODY()
public:
	virtual void BeginPlay() override;
	UPROPERTY() TObjectPtr<AActor> Block;
	UPROPERTY() TObjectPtr<UMaterialInstanceDynamic> Material;
	float Wait = 0.1f;
private:
	void Wake();
	FLinearColor Base;
	int32 Count = 0;
	FTimerHandle Timer;
};

/** Lifts 100 drops back up every second. */
UCLASS()
class AXBLifter : public AActor
{
	GENERATED_BODY()
public:
	virtual void BeginPlay() override;
	UPROPERTY() TArray<TObjectPtr<AXBBody>> Drops;
private:
	void Lift();
	int32 N = 1;
	FTimerHandle Timer;
};
