#include "XB10kGameMode.h"
#include "XB10kActors.h"
#include "Camera/CameraActor.h"
#include "Camera/CameraComponent.h"
#include "Components/DirectionalLightComponent.h"
#include "Components/SkyLightComponent.h"
#include "Components/StaticMeshComponent.h"
#include "Engine/DirectionalLight.h"
#include "Engine/SkyLight.h"
#include "Components/SkyAtmosphereComponent.h"
#include "Engine/StaticMesh.h"
#include "Engine/World.h"
#include "GameFramework/PlayerController.h"
#include "Materials/MaterialInstanceDynamic.h"
#include "Misc/App.h"
#include "Misc/CoreDelegates.h"
#include "Misc/FileHelper.h"
#include "Misc/Paths.h"
#include "ProceduralMeshComponent.h"
#include "RHI.h"

namespace
{
	constexpr int32 Side = 95, Grid = 8899, Spinners = 100, Drops = 1000, Blinkers = 97;
	constexpr float Spacing = 4.f;
	constexpr int32 Warmup = 60, Measured = 600;
	constexpr float Cm = 100.f; // 1 stud = 100 cm

	// Spec coordinates are Y up; Unreal's are Z up.
	FVector ToUnreal(const FVector& V) { return FVector(V.X, V.Z, V.Y); }
}

AXB10kGameMode::AXB10kGameMode()
{
	DefaultPawnClass = nullptr; // no player: the camera is fixed
}

void AXB10kGameMode::StartPlay()
{
	Super::StartPlay();
	const double Start = FPlatformTime::Seconds();
	Build();
	BuildMs = (FPlatformTime::Seconds() - Start) * 1000.0;
	UE_LOG(LogTemp, Display, TEXT("XB10K_CHECK count=%d sum=%.2f"), CheckCount, CheckSum);
	Last = FPlatformTime::Seconds();
	BeginFrameHandle = FCoreDelegates::OnBeginFrame.AddUObject(this, &AXB10kGameMode::OnBeginFrame);
}

void AXB10kGameMode::EndPlay(const EEndPlayReason::Type Reason)
{
	FCoreDelegates::OnBeginFrame.Remove(BeginFrameHandle);
	Super::EndPlay(Reason);
}

void AXB10kGameMode::OnBeginFrame()
{
	const double Now = FPlatformTime::Seconds();
	const double Ms = (Now - Last) * 1000.0;
	Last = Now;
	++Frame;
	if (Frame == 1) { FirstFrameMs = Ms; return; }
	if (Frame <= Warmup + 1) return;
	Samples.Add(Ms);
	if (Samples.Num() == Measured)
	{
		Report();
		FCoreDelegates::OnBeginFrame.Remove(BeginFrameHandle);
		FPlatformMisc::RequestExit(false);
	}
}

void AXB10kGameMode::Report()
{
	Samples.Sort();
	double Total = 0.0;
	for (double S : Samples) Total += S;
	const double Median = (Samples[Measured / 2 - 1] + Samples[Measured / 2]) / 2.0;
	const int32 Gi = IConsoleManager::Get().FindConsoleVariable(TEXT("r.DynamicGlobalIlluminationMethod"))->GetInt();
	const int32 Vsm = IConsoleManager::Get().FindConsoleVariable(TEXT("r.Shadow.Virtual.Enable"))->GetInt();
	const int32 Aa = IConsoleManager::Get().FindConsoleVariable(TEXT("r.AntiAliasingMethod"))->GetInt();
	const FString Json = FString::Printf(
		TEXT("{\"engine\":\"Unreal\",\"version\":\"%s\",\"renderer\":\"%s gi=%d vsm=%d aa=%d\",")
		TEXT("\"frames\":%d,\"median_ms\":%.4f,\"mean_ms\":%.4f,\"p95_ms\":%.4f,\"worst_ms\":%.4f,")
		TEXT("\"fps_from_median\":%.1f,\"build_ms\":%.1f,\"first_frame_ms\":%.2f,\"fixed_timestep\":%s}"),
		*FEngineVersion::Current().ToString(), GDynamicRHI ? GDynamicRHI->GetName() : TEXT("?"), Gi, Vsm, Aa,
		Measured, Median, Total / Measured, Samples[FMath::CeilToInt(Measured * 0.95) - 1], Samples[Measured - 1],
		1000.0 / Median, BuildMs, FirstFrameMs, FApp::UseFixedTimeStep() ? TEXT("true") : TEXT("false"));
	UE_LOG(LogTemp, Display, TEXT("XB10K_RESULT %s"), *Json);
	FFileHelper::SaveStringToFile(Json + TEXT("\n"), *FPaths::Combine(FPaths::ProjectDir(), TEXT("xb10k-result.json")));
}

UMaterialInstanceDynamic* AXB10kGameMode::MaterialFor(const FLinearColor& Color, bool bOwn)
{
	if (!bOwn)
	{
		if (TObjectPtr<UMaterialInstanceDynamic>* Found = Materials.Find(Color)) return *Found;
	}
	UMaterialInstanceDynamic* M = UMaterialInstanceDynamic::Create(BaseMaterial, this);
	M->SetVectorParameterValue(FName("Color"), Color);
	if (!bOwn) Materials.Add(Color, M);
	return M;
}

AXBBody* AXB10kGameMode::AddBody(UClass* Class, const FString& Name, const FString& Shape,
	const FVector& Pos, const FVector& Size, UMaterialInstanceDynamic* Material)
{
	FActorSpawnParameters Params;
	Params.Name = FName(*Name);
	AXBBody* Body = GetWorld()->SpawnActor<AXBBody>(Class, FTransform::Identity, Params);
	UPrimitiveComponent* Prim = nullptr;
	FRotator Turn = FRotator::ZeroRotator;
	if (Shape == TEXT("Wedge"))
	{
		// Slope rises from the bottom front edge (-Z) to the top back edge
		// (+Z), in spec axes; drawn from both sides so winding can't hide it.
		UProceduralMeshComponent* Proc = NewObject<UProceduralMeshComponent>(Body, TEXT("Shape"));
		const FVector H = Size / 2.f;
		auto V = [&](float X, float Y, float Z) { return ToUnreal(FVector(X, Y, Z)) * Cm; };
		const FVector A = V(-H.X, -H.Y, -H.Z), B = V(H.X, -H.Y, -H.Z), C = V(-H.X, -H.Y, H.Z),
			D = V(H.X, -H.Y, H.Z), E = V(-H.X, H.Y, H.Z), F = V(H.X, H.Y, H.Z);
		const FVector Tris[8][3] = { {A, C, D}, {A, D, B}, {C, E, F}, {C, F, D}, {A, E, C}, {B, D, F}, {A, B, F}, {A, F, E} };
		TArray<FVector> Verts; TArray<int32> Idx; TArray<FVector> Normals;
		for (const auto& T : Tris)
		{
			const FVector N = FVector::CrossProduct(T[1] - T[0], T[2] - T[0]).GetSafeNormal();
			for (int32 Side2 = 0; Side2 < 2; ++Side2)
			{
				for (int32 K = 0; K < 3; ++K)
				{
					Idx.Add(Verts.Num());
					Verts.Add(T[Side2 == 0 ? K : 2 - K]);
					Normals.Add(Side2 == 0 ? N : -N);
				}
			}
		}
		Proc->bUseComplexAsSimpleCollision = false;
		Proc->CreateMeshSection(0, Verts, Idx, Normals, {}, {}, {}, true);
		Proc->AddCollisionConvexMesh({ A, B, C, D, E, F });
		Prim = Proc;
	}
	else
	{
		UStaticMeshComponent* Mesh = NewObject<UStaticMeshComponent>(Body, TEXT("Shape"));
		if (Shape == TEXT("Block"))
		{
			Mesh->SetStaticMesh(Cube);
			Mesh->SetRelativeScale3D(ToUnreal(Size));
		}
		else if (Shape == TEXT("Ball"))
		{
			Mesh->SetStaticMesh(Sphere);
			Mesh->SetRelativeScale3D(FVector(FMath::Min3(Size.X, Size.Y, Size.Z)));
		}
		else // Cylinder: along X, length size.x, diameter min(size.y, size.z)
		{
			const float Dia = FMath::Min(Size.Y, Size.Z);
			Mesh->SetStaticMesh(Cylinder);
			Mesh->SetRelativeScale3D(FVector(Dia, Dia, Size.X));
			Turn = FRotator(90.f, 0.f, 0.f); // the mesh's Z (its length) onto X
		}
		Prim = Mesh;
	}
	// Grid and floor are Static, as placed level geometry would be; spinners
	// and drops move, so they're Movable.
	const bool bMovable = Class != AXBBody::StaticClass() || Name.StartsWith(TEXT("D"));
	Prim->SetMobility(bMovable ? EComponentMobility::Movable : EComponentMobility::Static);
	Prim->SetMaterial(0, Material);
	Prim->SetCollisionProfileName(TEXT("BlockAll"));
	Prim->SetRelativeLocationAndRotation(ToUnreal(Pos) * Cm, Turn);
	Body->SetRootComponent(Prim);
	Prim->RegisterComponent();
	Body->Shape = Prim;
	++CheckCount;
	CheckSum += Pos.X + Pos.Y + Pos.Z + Size.X + Size.Y + Size.Z;
	return Body;
}

void AXB10kGameMode::Build()
{
	UWorld* World = GetWorld();
	Cube = LoadObject<UStaticMesh>(nullptr, TEXT("/Engine/BasicShapes/Cube.Cube"));
	Sphere = LoadObject<UStaticMesh>(nullptr, TEXT("/Engine/BasicShapes/Sphere.Sphere"));
	Cylinder = LoadObject<UStaticMesh>(nullptr, TEXT("/Engine/BasicShapes/Cylinder.Cylinder"));
	BaseMaterial = LoadObject<UMaterialInterface>(nullptr, TEXT("/Engine/BasicShapes/BasicShapeMaterial.BasicShapeMaterial"));

	// Sun shining from (0.5, 1, 0.25) in spec axes, plus the sky and sky
	// light every new Unreal level starts with.
	ADirectionalLight* Sun = World->SpawnActor<ADirectionalLight>();
	Sun->GetRootComponent()->SetMobility(EComponentMobility::Movable);
	Sun->SetActorRotation((-ToUnreal(FVector(0.5f, 1.f, 0.25f))).Rotation());
	Sun->GetLightComponent()->SetCastShadows(true);
	AActor* Sky = World->SpawnActor<AActor>();
	USkyAtmosphereComponent* Atmosphere = NewObject<USkyAtmosphereComponent>(Sky);
	Sky->SetRootComponent(Atmosphere);
	Atmosphere->RegisterComponent();
	ASkyLight* SkyLight = World->SpawnActor<ASkyLight>();
	SkyLight->GetLightComponent()->SetMobility(EComponentMobility::Movable);
	SkyLight->GetLightComponent()->bRealTimeCapture = true;
	SkyLight->GetLightComponent()->RecaptureSky();

	// Unreal's field of view is horizontal: 70 degrees vertical at 16:9.
	ACameraActor* Camera = World->SpawnActor<ACameraActor>();
	const FVector Eye = ToUnreal(FVector(-260.f, 180.f, -260.f)) * Cm;
	Camera->SetActorLocationAndRotation(Eye, (-Eye).Rotation());
	Camera->GetCameraComponent()->SetFieldOfView(FMath::RadiansToDegrees(2.f * FMath::Atan(FMath::Tan(FMath::DegreesToRadians(35.f)) * 16.f / 9.f)));
	Camera->GetCameraComponent()->SetConstraintAspectRatio(false);
	if (APlayerController* PC = World->GetFirstPlayerController()) PC->SetViewTarget(Camera);

	AddBody(AXBBody::StaticClass(), TEXT("Floor"), TEXT("Block"), FVector(0.f, -0.5f, 0.f),
		FVector(440.f, 1.f, 440.f), MaterialFor(FLinearColor(0.35f, 0.37f, 0.40f), false));

	TMap<int32, int32> BlinkTargets;
	for (int32 K = 0; K < Blinkers; ++K) BlinkTargets.Add(K * (Grid / Blinkers), K);
	const TCHAR* Shapes[] = { TEXT("Block"), TEXT("Cylinder"), TEXT("Wedge") };
	for (int32 I = 0; I < Grid + Spinners; ++I)
	{
		const float X = (I % Side) * Spacing - (Side - 1) * Spacing / 2.f;
		const float Z = (I / Side) * Spacing - (Side - 1) * Spacing / 2.f;
		const float H = 1.f + (I * 7 % 5);
		const float Hue = (I % 8) / 8.f;
		const FLinearColor Color(Hue, 1.f - Hue, 0.5f + Hue / 2.f);
		const FVector Size(2.f, H, 2.f), Pos(X, H / 2.f, Z);
		if (I >= Grid)
		{
			const int32 K = I - Grid;
			AXBSpinner* Spinner = Cast<AXBSpinner>(AddBody(AXBSpinner::StaticClass(),
				FString::Printf(TEXT("S%d"), K), TEXT("Block"), Pos, Size, MaterialFor(Color, false)));
			Spinner->Speed = 30.f + (K % 7) * 20.f;
		}
		else
		{
			const int32* K = BlinkTargets.Find(I);
			UMaterialInstanceDynamic* Mat = MaterialFor(Color, K != nullptr);
			AXBBody* Block = AddBody(AXBBody::StaticClass(), FString::Printf(TEXT("G%d"), I), Shapes[I % 3], Pos, Size, Mat);
			if (K)
			{
				FActorSpawnParameters Params;
				Params.bDeferConstruction = true;
				AXBBlinker* Blinker = World->SpawnActor<AXBBlinker>(AXBBlinker::StaticClass(), FTransform::Identity, Params);
				Blinker->Block = Block;
				Blinker->Material = Mat;
				Blinker->Wait = 0.1f + (*K % 5) * 0.1f;
				Blinker->FinishSpawning(FTransform::Identity);
			}
		}
	}

	FActorSpawnParameters LifterParams;
	LifterParams.bDeferConstruction = true;
	AXBLifter* Lifter = World->SpawnActor<AXBLifter>(AXBLifter::StaticClass(), FTransform::Identity, LifterParams);
	UMaterialInstanceDynamic* DropMat = MaterialFor(FLinearColor(0.95f, 0.6f, 0.2f), false);
	for (int32 J = 0; J < Drops; ++J)
	{
		const FVector Pos((J % 10) * 3.f - 13.5f, 12.f + (J / 100) * 3.f, ((J / 10) % 10) * 3.f - 13.5f);
		AXBBody* Drop = AddBody(AXBBody::StaticClass(), FString::Printf(TEXT("D%d"), J),
			J % 2 == 0 ? TEXT("Ball") : TEXT("Block"), Pos, FVector(1.5f), DropMat);
		Drop->Shape->SetSimulatePhysics(true);
		Lifter->Drops.Add(Drop);
	}
	Lifter->FinishSpawning(FTransform::Identity);
}
