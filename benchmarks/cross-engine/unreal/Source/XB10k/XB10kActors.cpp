#include "XB10kActors.h"
#include "Components/StaticMeshComponent.h"
#include "Materials/MaterialInstanceDynamic.h"
#include "TimerManager.h"
#include "Engine/World.h"

AXBBody::AXBBody()
{
	PrimaryActorTick.bCanEverTick = false;
}

AXBSpinner::AXBSpinner()
{
	PrimaryActorTick.bCanEverTick = true;
}

void AXBSpinner::Tick(float DeltaSeconds)
{
	Super::Tick(DeltaSeconds);
	Angle = FMath::Fmod(Angle + Speed * DeltaSeconds, 360.f);
	SetActorRotation(FRotator(0.f, Angle, 0.f));
}

void AXBBlinker::BeginPlay()
{
	Super::BeginPlay();
	Material->GetVectorParameterValue(FName("Color"), Base);
	GetWorldTimerManager().SetTimer(Timer, this, &AXBBlinker::Wake, Wait, true);
}

void AXBBlinker::Wake()
{
	++Count;
	// Up is Z in Unreal; heights are in centimetres (1 stud = 100 cm).
	const float Height = Block->GetActorLocation().Z;
	Material->SetVectorParameterValue(FName("Color"),
		(Count % 2 == 0 && Height > 0.f) ? Base : FLinearColor(1.f, 0.5f, 0.f));
}

void AXBLifter::BeginPlay()
{
	Super::BeginPlay();
	GetWorldTimerManager().SetTimer(Timer, this, &AXBLifter::Lift, 1.f, true);
}

void AXBLifter::Lift()
{
	for (int32 I = 0; I < 100; ++I)
	{
		AXBBody* Drop = Drops[N - 1];
		const float X = (N % 10) * 3.f - 13.5f;
		const float Z = ((N / 10) % 10) * 3.f - 13.5f;
		// Spec (x, y, z), Y up -> Unreal (x, z, y) in centimetres.
		Drop->SetActorLocation(FVector(X, Z, 40.f) * 100.f, false, nullptr, ETeleportType::ResetPhysics);
		Drop->Shape->SetPhysicsLinearVelocity(FVector::ZeroVector);
		Drop->Shape->SetPhysicsAngularVelocityInDegrees(FVector::ZeroVector);
		N = N % Drops.Num() + 1;
	}
}
