use axcenity_core::{ClassName, Color3, Value, Vector3, World};
use axcenity_script::{OutputKind, ScriptRuntime};

use super::*;

fn text(value: &str) -> Expression {
    Expression::Text {
        value: value.to_owned(),
    }
}

fn number(value: f64) -> Expression {
    Expression::Number { value }
}

fn boxed(expression: Expression) -> Box<Expression> {
    Box::new(expression)
}

fn name(name: &str) -> Expression {
    Expression::Name {
        name: name.to_owned(),
    }
}

fn variable(name: &str) -> Expression {
    Expression::Variable {
        name: name.to_owned(),
    }
}

fn block(id: u64, kind: BlockKind) -> Block {
    Block {
        id,
        kind,
        ..Block::new(BlockKind::StopScript)
    }
}

fn script(blocks: Vec<Block>) -> BlockScript {
    BlockScript {
        blocks,
        ..Default::default()
    }
}

/// Which variant `kind` is. There's no `_ =>`, so a new block type stops
/// this compiling until it's added to [`every_statement`] and here.
fn statement_index(kind: &BlockKind) -> Option<usize> {
    Some(match kind {
        BlockKind::WhenStarted { .. }
        | BlockKind::WhenTouched { .. }
        | BlockKind::WhenKeyPressed { .. }
        | BlockKind::WhenClicked { .. }
        | BlockKind::WhenMouseEnters { .. }
        | BlockKind::WhenMouseLeaves { .. }
        | BlockKind::WhenValueChanged { .. }
        | BlockKind::EveryFrame { .. }
        | BlockKind::BeforePhysics { .. }
        | BlockKind::BeforeRender { .. }
        | BlockKind::WhenEventFired { .. }
        | BlockKind::WhenSoundEnded { .. }
        | BlockKind::WhenCutsceneFinished { .. }
        | BlockKind::WhenSplineReached { .. }
        | BlockKind::WhenConnectionFails { .. }
        | BlockKind::WhenStreamedIn { .. }
        | BlockKind::WhenStateChanged { .. }
        | BlockKind::WhenAnimationEvent { .. }
        | BlockKind::WhenAgentSenses { .. }
        | BlockKind::WhenSeatOccupied { .. }
        | BlockKind::WhenPromptTriggered { .. }
        | BlockKind::WhenPartClicked { .. }
        | BlockKind::WhenCommandRun { .. }
        | BlockKind::Unattached { .. }
        | BlockKind::TopCode { .. }
        | BlockKind::RunScript { .. }
        | BlockKind::OnSignal { .. } => return None,
        BlockKind::Wait { .. } => 0,
        BlockKind::WaitUntil { .. } => 1,
        BlockKind::Repeat { .. } => 2,
        BlockKind::Forever { .. } => 3,
        BlockKind::RepeatUntil { .. } => 4,
        BlockKind::If { .. } => 5,
        BlockKind::IfElse { .. } => 6,
        BlockKind::StopScript => 7,
        BlockKind::MoveTo { .. } => 8,
        BlockKind::MoveBy { .. } => 9,
        BlockKind::TurnTo { .. } => 10,
        BlockKind::TurnBy { .. } => 11,
        BlockKind::SetAnchored { .. } => 12,
        BlockKind::SetColor { .. } => 13,
        BlockKind::SetText { .. } => 14,
        BlockKind::Show { .. } => 15,
        BlockKind::Hide { .. } => 16,
        BlockKind::SetProperty { .. } => 17,
        BlockKind::PlaySound { .. } => 18,
        BlockKind::StopSound { .. } => 19,
        BlockKind::PauseSound { .. } => 20,
        BlockKind::ResumeSound { .. } => 21,
        BlockKind::SetVolume { .. } => 22,
        BlockKind::SetVariable { .. } => 23,
        BlockKind::ChangeVariable { .. } => 24,
        BlockKind::Print { .. } => 25,
        BlockKind::Destroy { .. } => 26,
        BlockKind::ShowPage { .. } => 27,
        BlockKind::Code { .. } => 28,
        BlockKind::SetAnimatorBool { .. } => 29,
        BlockKind::SetAnimatorNumber { .. } => 30,
        BlockKind::SetAnimatorTrigger { .. } => 31,
        BlockKind::ResetAnimatorTrigger { .. } => 32,
        BlockKind::PlayAnimatorState { .. } => 33,
        BlockKind::PlayAnimation { .. } => 34,
        BlockKind::Tween { .. } => 35,
        BlockKind::SlideIn { .. } => 36,
        BlockKind::FadeIn { .. } => 37,
        BlockKind::FadeOut { .. } => 38,
        BlockKind::ScrollTo { .. } => 39,
        BlockKind::SetValue { .. } => 40,
        BlockKind::SetDestination { .. } => 41,
        BlockKind::SetVelocity { .. } => 42,
        BlockKind::SaveSlot { .. } => 43,
        BlockKind::LoadSlot { .. } => 44,
        BlockKind::SetCheckpoint { .. } => 45,
        BlockKind::RestoreCheckpoint => 46,
        BlockKind::SetToneMapping { .. } => 47,
        BlockKind::SetGraphicsQuality { .. } => 48,
        BlockKind::SetGraphicsOption { .. } => 49,
        BlockKind::SetGraphicsNumber { .. } => 50,
        BlockKind::SetLanguage { .. } => 51,
        BlockKind::ForEachChild { .. } => 52,
        BlockKind::WaitForChild { .. } => 53,
        BlockKind::CloneInto { .. } => 54,
        BlockKind::LookAt { .. } => 55,
        BlockKind::MoveForward { .. } => 56,
        BlockKind::SetCameraType { .. } => 57,
        BlockKind::PointCameraAt { .. } => 58,
        BlockKind::ShakeCamera { .. } => 59,
        BlockKind::DestroyAfter { .. } => 60,
        BlockKind::FireEvent { .. } => 61,
        BlockKind::PushWithForce { .. } => 62,
        BlockKind::SetPartVelocity { .. } => 63,
        BlockKind::SetFriction { .. } => 64,
        BlockKind::ForEachPartInRadius { .. } => 65,
        BlockKind::ForEachPartTouching { .. } => 66,
        BlockKind::SetClockTime { .. } => 67,
        BlockKind::SetAtmosphere { .. } => 68,
        BlockKind::LeaveSeat { .. } => 69,
        BlockKind::LoadLevel { .. } => 70,
        BlockKind::AddLevel { .. } => 71,
        BlockKind::RemoveLevel { .. } => 72,
        BlockKind::PlayCutscene { .. } => 73,
        BlockKind::HostGame { .. } => 74,
        BlockKind::JoinGame { .. } => 75,
        BlockKind::LeaveGame => 76,
        BlockKind::SetNetworkOwner { .. } => 77,
        BlockKind::SetNetworkOwnershipAuto { .. } => 78,
        BlockKind::KickPlayer { .. } => 79,
        BlockKind::RequestStreamAround { .. } => 86,
        BlockKind::SendChatMessage { .. } => 80,
        BlockKind::ShowSystemMessage { .. } => 81,
        BlockKind::PlaceOnSpline { .. } => 82,
        BlockKind::Patrol { .. } => 83,
        BlockKind::SpawnDecal { .. } => 84,
        BlockKind::SetAnimatorLayerWeight { .. } => 85,
        BlockKind::Connect { .. } => 87,
        BlockKind::Comment { .. } => 88,
        BlockKind::Local { .. } => 89,
        BlockKind::Assign { .. } => 90,
        BlockKind::CompoundAssign { .. } => 91,
        BlockKind::CallFunction { .. } => 92,
        BlockKind::CallMethod { .. } => 93,
        BlockKind::DefineFunction { .. } => 94,
        BlockKind::While { .. } => 95,
        BlockKind::RepeatLoop { .. } => 96,
        BlockKind::NumericFor { .. } => 97,
        BlockKind::GenericFor { .. } => 98,
        BlockKind::DoBlock { .. } => 99,
        BlockKind::Break => 100,
        BlockKind::Continue => 101,
        BlockKind::Return { .. } => 102,
    })
}
const STATEMENT_COUNT: usize = 103;

/// One of each statement block, with the Lua it must produce.
fn every_statement() -> Vec<(BlockKind, &'static str)> {
    let pad = || "Pad".to_owned();
    let truth = || Expression::Boolean { value: true };
    vec![
        (
            BlockKind::Wait {
                seconds: number(0.5),
            },
            "task.wait(0.5)",
        ),
        (
            BlockKind::WaitUntil { condition: truth() },
            "while not (true) do",
        ),
        (
            BlockKind::Repeat {
                count: number(3.0),
                body: Vec::new(),
            },
            "for _ = 1, 3 do",
        ),
        (BlockKind::Forever { body: Vec::new() }, "while true do"),
        (
            BlockKind::RepeatUntil {
                condition: truth(),
                body: Vec::new(),
            },
            "while not (true) do",
        ),
        (
            BlockKind::If {
                condition: truth(),
                body: Vec::new(),
            },
            "if true then",
        ),
        (
            BlockKind::IfElse {
                condition: truth(),
                then_body: Vec::new(),
                else_body: vec![block(900, BlockKind::StopScript)],
            },
            "else",
        ),
        (BlockKind::StopScript, "do return end"),
        (
            BlockKind::MoveTo {
                target: pad(),
                position: Expression::Vector3 {
                    x: 1.0,
                    y: 2.0,
                    z: 3.0,
                },
            },
            "workspace.Pad.Position = Vector3.new(1, 2, 3)",
        ),
        (
            BlockKind::MoveBy {
                target: pad(),
                offset: Expression::Vector3 {
                    x: 0.0,
                    y: 1.0,
                    z: 0.0,
                },
            },
            "workspace.Pad.Position = workspace.Pad.Position + Vector3.new(0, 1, 0)",
        ),
        (
            BlockKind::TurnTo {
                target: pad(),
                orientation: Expression::Vector3 {
                    x: 0.0,
                    y: 90.0,
                    z: 0.0,
                },
            },
            "workspace.Pad.Orientation = Vector3.new(0, 90, 0)",
        ),
        (
            BlockKind::TurnBy {
                target: pad(),
                rotation: Expression::Vector3 {
                    x: 0.0,
                    y: 5.0,
                    z: 0.0,
                },
            },
            "workspace.Pad.Orientation = workspace.Pad.Orientation + Vector3.new(0, 5, 0)",
        ),
        (
            BlockKind::LookAt {
                target: pad(),
                position: Expression::Property {
                    target: "Goal".to_owned(),
                    property: "Position".to_owned(),
                },
            },
            "workspace.Pad.CFrame = CFrame.lookAt(workspace.Pad.Position, workspace.Goal.Position)",
        ),
        (
            BlockKind::MoveForward {
                target: pad(),
                distance: Expression::Math {
                    op: MathOp::Multiply,
                    left: boxed(variable("speed")),
                    right: boxed(number(2.0)),
                },
            },
            "workspace.Pad.Position = workspace.Pad.Position + workspace.Pad.CFrame.LookVector * (speed * 2)",
        ),
        (
            BlockKind::SetAnchored {
                target: pad(),
                anchored: Expression::Boolean { value: false },
            },
            "workspace.Pad.Anchored = false",
        ),
        (
            BlockKind::SetNetworkOwner {
                target: pad(),
                // Any value: here the `score` variable, which the block
                // script sets.
                owner: variable("score"),
            },
            "workspace.Pad:SetNetworkOwner(score)",
        ),
        (
            BlockKind::SetNetworkOwnershipAuto { target: pad() },
            "workspace.Pad:SetNetworkOwnershipAuto()",
        ),
        (
            BlockKind::KickPlayer {
                player: variable("score"),
                reason: Expression::Text {
                    value: "Cheating".to_owned(),
                },
            },
            "score:Kick(\"Cheating\")",
        ),
        (
            BlockKind::RequestStreamAround {
                player: variable("score"),
                position: Expression::Vector3 {
                    x: 0.0,
                    y: 0.0,
                    z: 500.0,
                },
            },
            "score:RequestStreamAroundAsync(Vector3.new(0, 0, 500))",
        ),
        (
            BlockKind::SendChatMessage {
                text: Expression::Text {
                    value: "Hello".to_owned(),
                },
            },
            "TextChatService:SendMessage(\"Hello\")",
        ),
        (
            BlockKind::ShowSystemMessage {
                text: Expression::Text {
                    value: "Round over".to_owned(),
                },
            },
            "TextChatService:DisplaySystemMessage(\"Round over\")",
        ),
        (
            BlockKind::PushWithForce {
                target: pad(),
                force: Expression::Vector3 {
                    x: 0.0,
                    y: 50.0,
                    z: 0.0,
                },
            },
            "workspace.Pad:ApplyImpulse(Vector3.new(0, 50, 0))",
        ),
        (
            BlockKind::SetPartVelocity {
                target: pad(),
                velocity: Expression::Vector3 {
                    x: 10.0,
                    y: 0.0,
                    z: 0.0,
                },
            },
            "workspace.Pad.AssemblyLinearVelocity = Vector3.new(10, 0, 0)",
        ),
        (
            BlockKind::SetFriction {
                target: pad(),
                friction: number(0.1),
            },
            "workspace.Pad.CustomPhysicalProperties = PhysicalProperties.new(physical.Density, \
             friction, physical.Elasticity, physical.FrictionWeight, physical.ElasticityWeight)",
        ),
        (
            BlockKind::SetColor {
                target: pad(),
                color: Expression::Color3 {
                    r: 1.0,
                    g: 0.0,
                    b: 0.0,
                },
            },
            "workspace.Pad.Color = Color3.new(1, 0, 0)",
        ),
        (
            BlockKind::SetText {
                target: "Hud.Score".to_owned(),
                text: variable("score"),
            },
            "workspace.Hud.Score.Text = tostring(score)",
        ),
        (
            BlockKind::Show { target: pad() },
            "workspace.Pad.Visible = true",
        ),
        (
            BlockKind::Hide { target: pad() },
            "workspace.Pad.Visible = false",
        ),
        (
            BlockKind::SetProperty {
                target: pad(),
                property: "Roughness".to_owned(),
                value: number(0.25),
            },
            "workspace.Pad.Roughness = 0.25",
        ),
        (
            BlockKind::PlaySound {
                target: "Pad.Chime".to_owned(),
            },
            "workspace.Pad.Chime:Play()",
        ),
        (
            BlockKind::StopSound {
                target: "Music".to_owned(),
            },
            "workspace.Music:Stop()",
        ),
        (
            BlockKind::PauseSound {
                target: "Music".to_owned(),
            },
            "workspace.Music:Pause()",
        ),
        (
            BlockKind::ResumeSound {
                target: "Music".to_owned(),
            },
            "workspace.Music:Resume()",
        ),
        (
            BlockKind::SetVolume {
                target: "Music".to_owned(),
                volume: number(0.5),
            },
            "workspace.Music.Volume = 0.5",
        ),
        (
            BlockKind::SetVariable {
                name: "score".to_owned(),
                value: number(10.0),
            },
            "score = 10",
        ),
        (
            BlockKind::ChangeVariable {
                name: "score".to_owned(),
                by: number(1.0),
            },
            "score = score + 1",
        ),
        (
            BlockKind::Print {
                value: Expression::Text {
                    value: "hi".to_owned(),
                },
            },
            "print(\"hi\")",
        ),
        (
            BlockKind::Destroy {
                target: "Red Ball".to_owned(),
            },
            "workspace[\"Red Ball\"]:Destroy()",
        ),
        (
            BlockKind::ForEachChild {
                target: "Coins".to_owned(),
                variable: "coin".to_owned(),
                body: Vec::new(),
            },
            "for _, coin in workspace.Coins:GetChildren() do",
        ),
        (
            BlockKind::ForEachPartInRadius {
                center: Expression::Vector3 {
                    x: 0.0,
                    y: 5.0,
                    z: 0.0,
                },
                radius: number(12.0),
                variable: "nearby".to_owned(),
                body: Vec::new(),
            },
            "for _, nearby in workspace:GetPartBoundsInRadius(Vector3.new(0, 5, 0), 12) do",
        ),
        (
            BlockKind::ForEachPartTouching {
                target: "Zone".to_owned(),
                variable: "inside".to_owned(),
                body: Vec::new(),
            },
            "for _, inside in workspace:GetPartsInPart(workspace.Zone) do",
        ),
        (
            BlockKind::WaitForChild {
                target: String::new(),
                name: "Door".to_owned(),
            },
            "workspace:WaitForChild(\"Door\")",
        ),
        (
            BlockKind::CloneInto {
                target: "Coin".to_owned(),
                parent: "Coins".to_owned(),
            },
            "workspace.Coin:Clone().Parent = workspace.Coins",
        ),
        (
            BlockKind::ShowPage {
                layout: "Menu".to_owned(),
                page: Expression::Text {
                    value: "Home".to_owned(),
                },
            },
            "workspace.Menu.CurrentPage = \"Home\"",
        ),
        (
            BlockKind::ScrollTo {
                target: "List".to_owned(),
                x: number(0.0),
                y: number(120.0),
            },
            "workspace.List.CanvasPosition = Vector2.new(0, 120)",
        ),
        (
            BlockKind::SetValue {
                target: "Settings.Volume".to_owned(),
                value: number(75.0),
            },
            "workspace.Settings.Volume.Value = 75",
        ),
        (
            BlockKind::SetDestination {
                agent: "Guard.NavigationAgent".to_owned(),
                position: Expression::Vector3 {
                    x: 20.0,
                    y: 0.0,
                    z: -5.0,
                },
            },
            "workspace.Guard.NavigationAgent.Destination = Vector3.new(20, 0, -5)",
        ),
        (
            BlockKind::LeaveSeat {
                controller: "Character.ThirdPersonController".to_owned(),
            },
            "workspace.Character.ThirdPersonController.Sit = false",
        ),
        (
            BlockKind::SetVelocity {
                agent: "Guard.NavigationAgent".to_owned(),
                velocity: Expression::Vector3 {
                    x: 0.0,
                    y: 0.0,
                    z: 8.0,
                },
            },
            "workspace.Guard.NavigationAgent.Velocity = Vector3.new(0, 0, 8)",
        ),
        (
            BlockKind::SaveSlot { slot: number(1.0) },
            "SaveService:SaveSlot(1)",
        ),
        (
            BlockKind::LoadSlot { slot: number(2.0) },
            "SaveService:LoadSlot(2)",
        ),
        (
            BlockKind::SetCheckpoint {
                checkpoint: "Checkpoints.Cave".to_owned(),
            },
            "SaveService:SetCheckpoint(workspace.Checkpoints.Cave)",
        ),
        (
            BlockKind::RestoreCheckpoint,
            "SaveService:RestoreCheckpoint()",
        ),
        (
            BlockKind::LoadLevel {
                level: text("Castle"),
                spawn: text("Gate"),
            },
            "LevelService:LoadLevel(\"Castle\", \"Gate\")",
        ),
        (
            BlockKind::AddLevel {
                level: text("Dungeon"),
            },
            "LevelService:AddLevel(\"Dungeon\")",
        ),
        (
            BlockKind::RemoveLevel {
                level: text("Dungeon"),
            },
            "LevelService:RemoveLevel(\"Dungeon\")",
        ),
        (
            BlockKind::HostGame {
                port: number(47800.0),
            },
            "NetworkService:Host(47800)",
        ),
        (
            BlockKind::JoinGame {
                address: text("192.168.1.20:47800"),
            },
            "NetworkService:Join(\"192.168.1.20:47800\")",
        ),
        (BlockKind::LeaveGame, "NetworkService:Disconnect()"),
        (
            BlockKind::PlayCutscene {
                sequence: "Intro".to_owned(),
            },
            "workspace.Intro:Play()",
        ),
        (
            BlockKind::PlaceOnSpline {
                target: "Cart".to_owned(),
                spline: "Track".to_owned(),
                distance: number(12.5),
            },
            "workspace.Cart:PivotTo(workspace.Track:GetCFrameAt(12.5))",
        ),
        (
            BlockKind::Patrol {
                agent: "Guard.NavigationAgent".to_owned(),
                spline: "Route".to_owned(),
            },
            "workspace.Guard.NavigationAgent:Patrol(workspace.Route)",
        ),
        (
            BlockKind::SetToneMapping {
                curve: "Filmic".to_owned(),
            },
            "Lighting.ToneMapping = Enum.ToneMapping.Filmic",
        ),
        (
            BlockKind::SetClockTime { time: number(18.5) },
            "Lighting.ClockTime = 18.5",
        ),
        (
            BlockKind::SetAtmosphere {
                property: "Haze".to_owned(),
                value: number(2.0),
            },
            "Lighting.Atmosphere.Haze = 2",
        ),
        (
            BlockKind::SetLanguage {
                language: Expression::Text {
                    value: "Français".to_owned(),
                },
            },
            "LocalizationService.Language = \"Français\"",
        ),
        (
            BlockKind::SetGraphicsQuality {
                quality: "Low".to_owned(),
            },
            "UserGameSettings.GraphicsQuality = Enum.GraphicsQuality.Low",
        ),
        (
            BlockKind::SetGraphicsOption {
                setting: "AntiAliasing".to_owned(),
                choice: "FXAA".to_owned(),
            },
            "UserGameSettings.AntiAliasing = Enum.AntiAliasing.FXAA",
        ),
        (
            BlockKind::SetGraphicsNumber {
                setting: "ShadowDistance".to_owned(),
                value: number(80.0),
            },
            "UserGameSettings.ShadowDistance = 80",
        ),
        (
            BlockKind::Code {
                code: "for i = 1, 2 do\n    print(i)\nend".to_owned(),
            },
            "    for i = 1, 2 do\n        print(i)\n    end",
        ),
        (
            BlockKind::SetAnimatorBool {
                animator: animator(),
                parameter: "IsGrounded".to_owned(),
                value: truth(),
            },
            "workspace.Hero.Animator:SetBool(\"IsGrounded\", true)",
        ),
        (
            BlockKind::SetAnimatorNumber {
                animator: animator(),
                parameter: "Speed".to_owned(),
                value: number(4.0),
            },
            "workspace.Hero.Animator:SetNumber(\"Speed\", 4)",
        ),
        (
            BlockKind::SetAnimatorLayerWeight {
                animator: animator(),
                layer: "UpperBody".to_owned(),
                weight: number(1.0),
                seconds: number(0.25),
            },
            "workspace.Hero.Animator:SetLayerWeight(\"UpperBody\", 1, 0.25)",
        ),
        (
            BlockKind::SetAnimatorTrigger {
                animator: animator(),
                parameter: "Jump".to_owned(),
            },
            "workspace.Hero.Animator:SetTrigger(\"Jump\")",
        ),
        (
            BlockKind::ResetAnimatorTrigger {
                animator: animator(),
                parameter: "Jump".to_owned(),
            },
            "workspace.Hero.Animator:ResetTrigger(\"Jump\")",
        ),
        (
            BlockKind::PlayAnimatorState {
                animator: animator(),
                state: "Jump".to_owned(),
            },
            "workspace.Hero.Animator:Play(\"Jump\")",
        ),
        (
            BlockKind::PlayAnimation {
                animator: animator(),
                animation: "Hero.Wave".to_owned(),
            },
            "workspace.Hero.Animator:LoadAnimation(workspace.Hero.Wave):Play()",
        ),
        (
            BlockKind::Tween {
                target: pad(),
                property: "Position".to_owned(),
                value: Expression::Vector3 {
                    x: 0.0,
                    y: 5.0,
                    z: 0.0,
                },
                seconds: number(1.0),
                style: "Quad".to_owned(),
                direction: "Out".to_owned(),
            },
            "TweenService:Create(workspace.Pad, TweenInfo.new(1, Enum.EasingStyle.Quad, \
             Enum.EasingDirection.Out), {Position = Vector3.new(0, 5, 0)}):Play()",
        ),
        (
            BlockKind::SlideIn {
                target: "Hud.Menu".to_owned(),
                from: "Left".to_owned(),
                seconds: number(0.5),
            },
            "workspace.Hud.Menu.Position = goal - UDim2.fromScale(1, 0)",
        ),
        (
            BlockKind::FadeIn {
                target: "Hud.Menu".to_owned(),
                seconds: number(0.5),
            },
            "TweenService:Create(workspace.Hud.Menu, TweenInfo.new(0.5), \
             {BackgroundTransparency = 0}):Play()",
        ),
        (
            BlockKind::FadeOut {
                target: "Hud.Menu".to_owned(),
                seconds: number(0.5),
            },
            "TweenService:Create(workspace.Hud.Menu, TweenInfo.new(0.5), \
             {BackgroundTransparency = 1}):Play()",
        ),
        (
            BlockKind::SetCameraType {
                camera_type: "Scriptable".to_owned(),
            },
            "workspace.CurrentCamera.CameraType = Enum.CameraType.Scriptable",
        ),
        (
            BlockKind::PointCameraAt {
                position: Expression::Vector3 {
                    x: 0.0,
                    y: 2.0,
                    z: 0.0,
                },
            },
            "workspace.CurrentCamera.CFrame = \
             CFrame.lookAt(workspace.CurrentCamera.Position, Vector3.new(0, 2, 0))",
        ),
        (
            BlockKind::ShakeCamera {
                intensity: number(1.5),
                seconds: number(0.5),
            },
            "workspace.CurrentCamera:Shake(1.5, 0.5)",
        ),
        (
            BlockKind::DestroyAfter {
                target: "Spark".to_owned(),
                seconds: number(2.0),
            },
            "Debris:AddItem(workspace.Spark, 2)",
        ),
        (
            BlockKind::SpawnDecal {
                texture: text("assets/textures/hole.png"),
                position: Expression::Math {
                    op: MathOp::Add,
                    left: boxed(variable("hit")),
                    right: boxed(Expression::Vector3 {
                        x: 0.0,
                        y: 1.0,
                        z: 0.0,
                    }),
                },
                facing: Expression::Vector3 {
                    x: 0.0,
                    y: 1.0,
                    z: 0.0,
                },
                size: number(0.5),
                seconds: number(20.0),
            },
            "workspace:SpawnDecal(\"assets/textures/hole.png\", CFrame.lookAt(Vector3.new(), Vector3.new(0, 1, 0)) + (hit + Vector3.new(0, 1, 0)), 0.5, 20)",
        ),
        (
            BlockKind::FireEvent {
                event: "Events.Scored".to_owned(),
            },
            "game.ReplicatedStorage.Events.Scored:Fire()",
        ),
        (
            BlockKind::Connect {
                signal: name("button.Activated"),
                method: "Once".to_owned(),
                params: "player".to_owned(),
                body: Vec::new(),
            },
            "button.Activated:Once(function(player)\n    end)",
        ),
        (
            BlockKind::Comment {
                text: "-- A note".to_owned(),
            },
            "-- A note",
        ),
        (
            BlockKind::Local {
                names: "a, b".to_owned(),
                values: vec![number(1.0), name("c")],
            },
            "local a, b = 1, c",
        ),
        (
            BlockKind::Assign {
                targets: vec![name("a.b")],
                values: vec![Expression::Nil],
            },
            "a.b = nil",
        ),
        (
            BlockKind::CompoundAssign {
                target: name("a"),
                op: "..".to_owned(),
                value: text("!"),
            },
            "a ..= \"!\"",
        ),
        (
            BlockKind::CallFunction {
                function: name("table.insert"),
                args: vec![name("list"), number(2.0)],
            },
            "table.insert(list, 2)",
        ),
        (
            BlockKind::CallMethod {
                object: name("gui"),
                method: "WaitForChild".to_owned(),
                args: vec![text("Panel")],
            },
            "gui:WaitForChild(\"Panel\")",
        ),
        (
            BlockKind::DefineFunction {
                name: "show".to_owned(),
                local: true,
                params: "row".to_owned(),
                generics: String::new(),
                returns: String::new(),
                attributes: String::new(),
                body: Vec::new(),
            },
            "local function show(row)\n    end",
        ),
        (
            BlockKind::While {
                condition: name("running"),
                body: Vec::new(),
            },
            "while running do\n    end",
        ),
        (
            BlockKind::RepeatLoop {
                body: Vec::new(),
                condition: name("done"),
            },
            "repeat\n    until done",
        ),
        (
            BlockKind::NumericFor {
                variable: "i".to_owned(),
                start: number(1.0),
                stop: number(9.0),
                step: Some(number(2.0)),
                body: Vec::new(),
            },
            "for i = 1, 9, 2 do\n    end",
        ),
        (
            BlockKind::GenericFor {
                names: "_, row".to_owned(),
                values: vec![Expression::Call {
                    function: boxed(name("ipairs")),
                    args: vec![name("rows")],
                }],
                body: Vec::new(),
            },
            "for _, row in ipairs(rows) do\n    end",
        ),
        (BlockKind::DoBlock { body: Vec::new() }, "do\n    end"),
        (BlockKind::Break, "break"),
        (BlockKind::Continue, "continue"),
        (
            BlockKind::Return {
                values: vec![name("x"), Expression::Vararg],
            },
            "return x, ...",
        ),
    ]
}

fn animator() -> String {
    "Hero.Animator".to_owned()
}

#[test]
fn translation_covers_every_statement_block() {
    let statements = every_statement();
    let mut covered = [false; STATEMENT_COUNT];
    let body = statements
        .iter()
        .enumerate()
        .map(|(index, (kind, _))| {
            covered[statement_index(kind).unwrap()] = true;
            block(index as u64 + 10, kind.clone())
        })
        .collect();
    assert!(covered.iter().all(|&c| c), "every_statement misses a block");

    let translated = translate(&script(vec![block(1, BlockKind::WhenStarted { body })])).unwrap();
    for (kind, lua) in &statements {
        assert!(
            translated.source.contains(lua),
            "{kind:?} should give `{lua}`:\n{}",
            translated.source
        );
    }
}

#[test]
fn translation_covers_every_event_block() {
    let say = |id| {
        vec![block(
            id,
            BlockKind::Print {
                value: Expression::TouchingPart,
            },
        )]
    };
    let quiet = |id| vec![block(id, BlockKind::StopScript)];
    let blocks = vec![
        block(1, BlockKind::WhenStarted { body: quiet(11) }),
        block(
            2,
            BlockKind::WhenTouched {
                part: "Pad".to_owned(),
                body: say(12),
            },
        ),
        block(
            3,
            BlockKind::WhenKeyPressed {
                key: "Space".to_owned(),
                body: quiet(13),
            },
        ),
        block(
            4,
            BlockKind::WhenClicked {
                button: "Menu.Start".to_owned(),
                body: quiet(14),
            },
        ),
        block(5, BlockKind::EveryFrame { body: quiet(15) }),
        block(
            6,
            BlockKind::WhenSoundEnded {
                sound: "Music".to_owned(),
                body: quiet(16),
            },
        ),
        block(7, BlockKind::Unattached { body: quiet(17) }),
        block(
            8,
            BlockKind::WhenStateChanged {
                animator: animator(),
                body: quiet(18),
            },
        ),
        block(
            9,
            BlockKind::WhenAnimationEvent {
                animator: animator(),
                event: "Footstep".to_owned(),
                body: quiet(19),
            },
        ),
        block(
            30,
            BlockKind::WhenMouseEnters {
                element: "Menu.Start".to_owned(),
                body: quiet(31),
            },
        ),
        block(
            32,
            BlockKind::WhenMouseLeaves {
                element: "Menu.Start".to_owned(),
                body: quiet(33),
            },
        ),
        block(
            34,
            BlockKind::WhenValueChanged {
                widget: "Settings.Volume".to_owned(),
                body: quiet(35),
            },
        ),
        block(
            36,
            BlockKind::WhenAgentSenses {
                agent: "Guard.NavigationAgent".to_owned(),
                sense: "Sight".to_owned(),
                body: quiet(37),
            },
        ),
        block(
            44,
            BlockKind::WhenSeatOccupied {
                seat: "Chair".to_owned(),
                body: quiet(45),
            },
        ),
        block(
            46,
            BlockKind::WhenPromptTriggered {
                prompt: "Door.ProximityPrompt".to_owned(),
                body: quiet(47),
            },
        ),
        block(
            48,
            BlockKind::WhenPartClicked {
                detector: "Button.ClickDetector".to_owned(),
                body: quiet(49),
            },
        ),
        block(
            94,
            BlockKind::WhenCommandRun {
                command: text("heal"),
                help: text("Heals the player"),
                body: quiet(95),
            },
        ),
        block(38, BlockKind::BeforePhysics { body: quiet(39) }),
        block(40, BlockKind::BeforeRender { body: quiet(41) }),
        block(
            42,
            BlockKind::WhenEventFired {
                event: "Scored".to_owned(),
                body: quiet(43),
            },
        ),
        block(
            90,
            BlockKind::WhenCutsceneFinished {
                sequence: "Intro".to_owned(),
                body: quiet(91),
            },
        ),
        block(92, BlockKind::WhenConnectionFails { body: quiet(93) }),
        block(
            96,
            BlockKind::WhenSplineReached {
                follower: "Lift.SplineFollower".to_owned(),
                which: "End".to_owned(),
                body: quiet(97),
            },
        ),
    ];
    let source = translate(&script(blocks)).unwrap().source;
    for lua in [
        "local RunService = game:GetService(\"RunService\")",
        "local UserInputService = game:GetService(\"UserInputService\")",
        "task.spawn(function()",
        "workspace.Pad.Touched:Connect(function(otherPart)",
        "    print(otherPart)",
        "UserInputService.InputBegan:Connect(function(input)",
        "    if input.KeyCode == Enum.KeyCode.Space then",
        "workspace.Menu.Start.Activated:Connect(function()",
        "RunService.Heartbeat:Connect(function(deltaTime)",
        "workspace.Music.Ended:Connect(function()",
        "workspace.Intro.Completed:Connect(function()",
        "workspace.Hero.Animator.StateChanged:Connect(function(state, previous)",
        "workspace.Hero.Animator.AnimationEvent:Connect(function(name)",
        "    if name == \"Footstep\" then",
        "workspace.Menu.Start.MouseEnter:Connect(function()",
        "workspace.Menu.Start.MouseLeave:Connect(function()",
        "workspace.Settings.Volume.ValueChanged:Connect(function()",
        "workspace.Guard.NavigationAgent.PlayerSensed:Connect(function(sense)",
        "    if sense == \"Sight\" then",
        "workspace.Chair:GetPropertyChangedSignal(\"Occupant\"):Connect(function()",
        "    if workspace.Chair.Occupant then",
        "workspace.Door.ProximityPrompt.Triggered:Connect(function()",
        "workspace.Button.ClickDetector.MouseClick:Connect(function()",
        "local ConsoleService = game:GetService(\"ConsoleService\")",
        "ConsoleService:RegisterCommand(\"heal\", \"Heals the player\", function()",
        "RunService.Stepped:Connect(function(time, deltaTime)",
        "RunService.RenderStepped:Connect(function(deltaTime)",
        "game.ReplicatedStorage.Scored.Event:Connect(function()",
        "local NetworkService = game:GetService(\"NetworkService\")",
        "NetworkService.ConnectionFailed:Connect(function(reason)",
        "workspace.Lift.SplineFollower.Reached:Connect(function(which)",
        "    if which == \"End\" then",
    ] {
        assert!(source.contains(lua), "missing `{lua}`:\n{source}");
    }
    // Unattached blocks don't run.
    assert_eq!(source.matches("do return end").count(), 21, "{source}");
}

#[test]
fn reporters_translate_with_brackets_where_the_blocks_nest() {
    let sum = Expression::Math {
        op: MathOp::Add,
        left: boxed(variable("a")),
        right: boxed(number(2.0)),
    };
    let cases = [
        (
            Expression::Math {
                op: MathOp::Multiply,
                left: boxed(sum.clone()),
                right: boxed(number(3.0)),
            },
            "(a + 2) * 3",
        ),
        (
            Expression::Function {
                function: MathFunction::Round,
                value: boxed(variable("a")),
            },
            "math.floor(a + 0.5)",
        ),
        (
            Expression::Random {
                min: boxed(number(1.0)),
                max: boxed(number(6.0)),
            },
            "math.random(math.floor(1), math.floor(6))",
        ),
        (
            Expression::RandomNumber {
                min: boxed(number(0.5)),
                max: boxed(variable("a")),
            },
            "Random.new():NextNumber(0.5, a)",
        ),
        (
            Expression::Lerp {
                from: boxed(Expression::Color3 {
                    r: 1.0,
                    g: 0.0,
                    b: 0.0,
                }),
                to: boxed(variable("a")),
                alpha: boxed(number(0.25)),
            },
            "Color3.new(1, 0, 0):Lerp(a, 0.25)",
        ),
        (
            Expression::Lerp {
                from: boxed(Expression::Logic {
                    op: LogicOp::Or,
                    left: boxed(variable("a")),
                    right: boxed(variable("b")),
                }),
                to: boxed(variable("b")),
                alpha: boxed(number(0.5)),
            },
            "(a or b):Lerp(b, 0.5)",
        ),
        (
            Expression::Join {
                left: boxed(Expression::Text {
                    value: "Score: ".to_owned(),
                }),
                right: boxed(variable("a")),
            },
            "tostring(\"Score: \") .. tostring(a)",
        ),
        (
            Expression::Not {
                value: boxed(Expression::Logic {
                    op: LogicOp::And,
                    left: boxed(Expression::Compare {
                        op: Comparison::Greater,
                        left: boxed(variable("a")),
                        right: boxed(number(1.0)),
                    }),
                    right: boxed(Expression::KeyDown {
                        key: "W".to_owned(),
                    }),
                }),
            },
            "not (a > 1 and UserInputService:IsKeyDown(Enum.KeyCode.W))",
        ),
        (
            Expression::Property {
                target: "Pad".to_owned(),
                property: "Position".to_owned(),
            },
            "workspace.Pad.Position",
        ),
    ];
    for (expression, lua) in cases {
        let blocks = vec![block(
            1,
            BlockKind::WhenStarted {
                body: vec![block(
                    2,
                    BlockKind::Print {
                        value: expression.clone(),
                    },
                )],
            },
        )];
        let source = translate(&script(blocks)).unwrap().source;
        assert!(
            source.contains(&format!("print({lua})")),
            "{expression:?} should give `{lua}`:\n{source}"
        );
    }
}

#[test]
fn variables_belong_to_the_whole_script_and_start_at_zero() {
    let blocks = vec![block(
        1,
        BlockKind::WhenStarted {
            body: vec![
                block(
                    2,
                    BlockKind::ChangeVariable {
                        name: "score".to_owned(),
                        by: number(1.0),
                    },
                ),
                block(
                    3,
                    BlockKind::Print {
                        value: variable("lives"),
                    },
                ),
            ],
        },
    )];
    let translated = translate(&script(blocks)).unwrap();
    assert_eq!(
        translated.source,
        "local lives = 0\nlocal score = 0\n\ntask.spawn(function()\n    score = score + 1\n    print(lives)\nend)\n"
    );
    // Lines count from 1: the event is on 4, its blocks on 5 and 6.
    assert_eq!(translated.block_lines, [(4, 1), (5, 2), (6, 3)]);
    assert_eq!(translated.block_at(6), Some(3));
    assert_eq!(translated.block_at(7), Some(3));
    assert_eq!(translated.block_at(2), None);
}

#[test]
fn translated_blocks_run_in_the_scheduler_like_typed_lua() {
    let mut world = World::new();
    let pad = world.create(ClassName::Part).unwrap();
    world.set_name(pad, "Pad").unwrap();
    world.set_parent(pad, Some(world.workspace())).unwrap();

    let blocks = vec![block(
        1,
        BlockKind::WhenStarted {
            body: vec![
                block(
                    2,
                    BlockKind::SetColor {
                        target: "Pad".to_owned(),
                        color: Expression::Color3 {
                            r: 1.0,
                            g: 0.0,
                            b: 0.0,
                        },
                    },
                ),
                block(
                    3,
                    BlockKind::Repeat {
                        count: number(3.0),
                        body: vec![
                            block(
                                4,
                                BlockKind::MoveBy {
                                    target: "Pad".to_owned(),
                                    offset: Expression::Vector3 {
                                        x: 0.0,
                                        y: 1.0,
                                        z: 0.0,
                                    },
                                },
                            ),
                            block(
                                5,
                                BlockKind::ChangeVariable {
                                    name: "moves".to_owned(),
                                    by: number(1.0),
                                },
                            ),
                        ],
                    },
                ),
                block(
                    6,
                    BlockKind::Wait {
                        seconds: number(1.0),
                    },
                ),
                block(
                    7,
                    BlockKind::Print {
                        value: Expression::Join {
                            left: boxed(Expression::Text {
                                value: "moves: ".to_owned(),
                            }),
                            right: boxed(variable("moves")),
                        },
                    },
                ),
            ],
        },
    )];
    let source = translate(&script(blocks)).unwrap().source;

    let lua = world.create(ClassName::Script).unwrap();
    world.set_name(lua, "Blocks").unwrap();
    world
        .set_parent(lua, Some(world.server_script_service()))
        .unwrap();
    let mut runtime = axcenity_gameplay::script_runtime();
    runtime.start_script(
        &mut world,
        lua,
        "scripts/Blocks.server.blocks.json",
        &source,
    );
    runtime.step(&mut world, 0.5);
    let property = |name| world.get(pad).unwrap().property(name).unwrap().clone();
    assert_eq!(property("Color"), Value::Color3(Color3::new(1.0, 0.0, 0.0)));
    assert_eq!(
        property("Position"),
        Value::Vector3(Vector3::new(0.0, 3.0, 0.0))
    );
    assert!(runtime.take_output().is_empty(), "the wait isn't over yet");

    runtime.step(&mut world, 1.0);
    let output: Vec<_> = runtime
        .take_output()
        .into_iter()
        .map(|line| (line.kind, line.text))
        .collect();
    assert_eq!(output, [(OutputKind::Print, "moves: 3".to_owned())]);
}

/// The When Command Run block registers a Developer Console command that
/// runs its blocks when typed (Phase 71).
#[test]
fn a_when_command_run_block_runs_its_blocks_when_the_command_is_typed() {
    let blocks = vec![block(
        1,
        BlockKind::WhenCommandRun {
            command: text("heal"),
            help: text("Heals the player"),
            body: vec![block(
                2,
                BlockKind::Print {
                    value: text("healed"),
                },
            )],
        },
    )];
    let source = translate(&script(blocks)).unwrap().source;
    let mut world = World::new();
    let lua = world.create(ClassName::Script).unwrap();
    world
        .set_parent(lua, Some(world.server_script_service()))
        .unwrap();
    let mut runtime = axcenity_gameplay::script_runtime();
    runtime.start_script(&mut world, lua, "scripts/Heal.server.blocks.json", &source);
    let commands = runtime.console_commands();
    assert_eq!(commands.len(), 1, "{source}");
    assert_eq!(commands[0].help, "Heals the player");
    assert_eq!(
        runtime.run_console_command(&mut world, "heal"),
        axcenity_script::ConsoleRun::Ran
    );
    let output: Vec<_> = runtime
        .take_output()
        .into_iter()
        .map(|line| line.text)
        .collect();
    assert_eq!(output, ["healed"]);
}

/// The time of day and atmosphere blocks set Lighting's ClockTime and its
/// Atmosphere's air, and come back from their Lua.
#[test]
fn the_time_of_day_and_atmosphere_blocks_set_lighting_and_come_back_from_lua() {
    let blocks = script(vec![block(
        1,
        BlockKind::WhenStarted {
            body: vec![
                block(2, BlockKind::SetClockTime { time: number(18.5) }),
                block(
                    3,
                    BlockKind::SetAtmosphere {
                        property: "Haze".to_owned(),
                        value: number(2.0),
                    },
                ),
            ],
        },
    )]);
    let source = translate(&blocks).unwrap().source;
    assert!(
        source.starts_with("local Lighting = game:GetService(\"Lighting\")"),
        "{source}"
    );

    let mut world = World::new();
    let atmosphere = world.create(ClassName::Atmosphere).unwrap();
    world
        .set_parent(atmosphere, Some(world.lighting()))
        .unwrap();
    let lua = world.create(ClassName::Script).unwrap();
    world
        .set_parent(lua, Some(world.server_script_service()))
        .unwrap();
    let mut runtime = axcenity_gameplay::script_runtime();
    runtime.start_script(&mut world, lua, "scripts/Day.server.blocks.json", &source);
    runtime.step(&mut world, 0.1);
    assert_eq!(
        world.get(world.lighting()).unwrap().property("ClockTime"),
        Some(&Value::Number(18.5))
    );
    assert_eq!(
        world.get(atmosphere).unwrap().property("Haze"),
        Some(&Value::Number(2.0))
    );

    let back = crate::from_lua(&source).unwrap();
    assert_eq!(translate(&back).unwrap().source, source);
    let mut kinds = Vec::new();
    back.for_each_block(&mut |b| kinds.push(b.kind.clone()));
    assert!(kinds.contains(&BlockKind::SetClockTime { time: number(18.5) }));
    assert!(kinds.contains(&BlockKind::SetAtmosphere {
        property: "Haze".to_owned(),
        value: number(2.0),
    }));

    let wrong = script(vec![block(
        1,
        BlockKind::WhenStarted {
            body: vec![block(
                2,
                BlockKind::SetAtmosphere {
                    property: "Color".to_owned(),
                    value: number(1.0),
                },
            )],
        },
    )]);
    assert!(translate(&wrong).is_err());
}

#[test]
fn the_tone_mapping_block_sets_lighting_and_comes_back_from_lua() {
    assert_eq!(
        crate::TONE_MAPPINGS,
        axcenity_core::ToneMapping::ALL.map(|curve| curve.as_str())
    );
    let blocks = script(vec![block(
        1,
        BlockKind::WhenStarted {
            body: vec![block(
                2,
                BlockKind::SetToneMapping {
                    curve: "Neutral".to_owned(),
                },
            )],
        },
    )]);
    let source = translate(&blocks).unwrap().source;
    assert!(
        source.starts_with("local Lighting = game:GetService(\"Lighting\")"),
        "{source}"
    );

    let mut world = World::new();
    let lua = world.create(ClassName::Script).unwrap();
    world
        .set_parent(lua, Some(world.server_script_service()))
        .unwrap();
    let mut runtime = axcenity_gameplay::script_runtime();
    runtime.start_script(&mut world, lua, "scripts/Look.server.blocks.json", &source);
    runtime.step(&mut world, 0.1);
    assert_eq!(
        world.get(world.lighting()).unwrap().property("ToneMapping"),
        Some(&Value::ToneMapping(axcenity_core::ToneMapping::Neutral))
    );

    let back = crate::from_lua(&source).unwrap();
    assert_eq!(translate(&back).unwrap().source, source);
    let mut kinds = Vec::new();
    back.for_each_block(&mut |b| kinds.push(b.kind.clone()));
    assert!(kinds.contains(&BlockKind::SetToneMapping {
        curve: "Neutral".to_owned()
    }));

    let wrong = script(vec![block(
        1,
        BlockKind::WhenStarted {
            body: vec![block(
                2,
                BlockKind::SetToneMapping {
                    curve: "Bright".to_owned(),
                },
            )],
        },
    )]);
    assert!(translate(&wrong).is_err());
}

#[test]
fn data_blocks_read_a_tables_row_and_column_and_come_back_from_lua() {
    let value = |column: &str, key: &str| Expression::DataValue {
        column: column.to_owned(),
        row: boxed(Expression::DataRow {
            table: "weapons".to_owned(),
            key: boxed(Expression::Text {
                value: key.to_owned(),
            }),
        }),
    };
    let print = |id, value| block(id, BlockKind::Print { value });
    let blocks = script(vec![block(
        1,
        BlockKind::WhenStarted {
            body: vec![
                print(2, value("Damage", "sword")),
                print(3, value("Kind", "bow")),
            ],
        },
    )]);
    let source = translate(&blocks).unwrap().source;
    assert!(
        source.starts_with("local DataService = game:GetService(\"DataService\")"),
        "{source}"
    );
    assert!(
        source.contains("print(DataService:GetTable(\"weapons\"):Get(\"sword\").Damage)"),
        "{source}"
    );

    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-data");
    let mut world = World::new();
    let lua = world.create(ClassName::Script).unwrap();
    world
        .set_parent(lua, Some(world.server_script_service()))
        .unwrap();
    let mut runtime = axcenity_gameplay::script_runtime();
    runtime.set_assets(axcenity_assets::Assets::new(folder));
    runtime.start_script(&mut world, lua, "scripts/Data.server.blocks.json", &source);
    runtime.step(&mut world, 0.1);
    let printed: Vec<String> = runtime
        .take_output()
        .into_iter()
        .map(|line| line.text)
        .collect();
    assert_eq!(printed, ["10", "Ranged"]);

    let back = crate::from_lua(&source).unwrap();
    assert!(!has_code(&back), "{back:#?}");
    assert_eq!(translate(&back).unwrap().source, source);
}

#[test]
fn graphics_blocks_change_user_game_settings_and_come_back_from_lua() {
    let run = |world: &mut World, runtime: &mut ScriptRuntime, path: &str, source: &str| {
        let lua = world.create(ClassName::LocalScript).unwrap();
        world
            .set_parent(lua, Some(world.starter_player_scripts()))
            .unwrap();
        runtime.start_script(world, lua, path, source);
        runtime.step(world, 0.1);
        runtime
            .take_output()
            .into_iter()
            .map(|line| line.text)
            .collect::<Vec<_>>()
    };
    // Every choice the menus offer is one the script API takes.
    let mut every = vec![];
    for quality in crate::GRAPHICS_QUALITIES {
        every.push(BlockKind::SetGraphicsQuality {
            quality: quality.to_owned(),
        });
    }
    for setting in crate::GRAPHICS_OPTIONS {
        assert_eq!(
            crate::graphics_choices(setting).last(),
            Some(&crate::PRESET)
        );
        for choice in crate::graphics_choices(setting) {
            every.push(BlockKind::SetGraphicsOption {
                setting: setting.to_owned(),
                choice: (*choice).to_owned(),
            });
        }
    }
    let body = every
        .into_iter()
        .enumerate()
        .map(|(index, kind)| block(index as u64 + 10, kind))
        .collect();
    let source = translate(&script(vec![block(1, BlockKind::WhenStarted { body })]))
        .unwrap()
        .source;
    let (mut world, mut runtime) = (World::new(), axcenity_gameplay::script_runtime());
    let output = run(
        &mut world,
        &mut runtime,
        "scripts/All.client.blocks.json",
        &source,
    );
    assert!(output.is_empty(), "{output:?}");

    let set = |id, kind| block(id, kind);
    let option = |setting: &str, choice: &str| BlockKind::SetGraphicsOption {
        setting: setting.to_owned(),
        choice: choice.to_owned(),
    };
    let blocks = script(vec![block(
        1,
        BlockKind::WhenStarted {
            body: vec![
                set(
                    2,
                    BlockKind::SetGraphicsQuality {
                        quality: "Medium".to_owned(),
                    },
                ),
                set(3, option("AntiAliasing", "MSAA2")),
                set(4, option("PostProcessing", "Off")),
                set(5, option("ShadowQuality", "Off")),
                set(6, option("ShadowQuality", "Preset")),
                set(8, option("ContactShadows", "On")),
                set(
                    7,
                    BlockKind::SetGraphicsNumber {
                        setting: "RenderScale".to_owned(),
                        value: number(0.5),
                    },
                ),
            ],
        },
    )]);
    let source = translate(&blocks).unwrap().source;
    assert!(
        source.starts_with("local UserGameSettings = game:GetService(\"UserGameSettings\")"),
        "{source}"
    );
    assert!(
        source.contains("UserGameSettings.PostProcessing = false"),
        "{source}"
    );
    assert!(
        source.contains("UserGameSettings.ShadowQuality = nil"),
        "{source}"
    );
    assert!(
        source.contains("UserGameSettings.ContactShadows = true"),
        "{source}"
    );

    let (mut world, mut runtime) = (World::new(), axcenity_gameplay::script_runtime());
    let output = run(
        &mut world,
        &mut runtime,
        "scripts/Menu.client.blocks.json",
        &source,
    );
    assert!(output.is_empty(), "{output:?}");
    let output = run(
        &mut world,
        &mut runtime,
        "scripts/check.client.lua",
        "local s = game:GetService('UserGameSettings')
        print(s.GraphicsQuality, s.AntiAliasing, s.PostProcessing, s.ShadowQuality, s.RenderScale)",
    );
    assert_eq!(
        output,
        ["Enum.GraphicsQuality.Medium	Enum.AntiAliasing.MSAA2	false	Enum.ShadowQuality.Medium	0.5"]
    );
    assert!(runtime.take_graphics_change().is_some());

    let back = crate::from_lua(&source).unwrap();
    assert_eq!(translate(&back).unwrap().source, source);
    let mut kinds = Vec::new();
    back.for_each_block(&mut |b| kinds.push(b.kind.clone()));
    assert!(kinds.contains(&option("PostProcessing", "Off")));
    assert!(kinds.contains(&option("ShadowQuality", "Preset")));
    assert!(kinds.contains(&option("ContactShadows", "On")));

    for wrong in [
        BlockKind::SetGraphicsQuality {
            quality: "Best".to_owned(),
        },
        option("Shadows", "Off"),
        option("AntiAliasing", "Quarter"),
        BlockKind::SetGraphicsNumber {
            setting: "Exposure".to_owned(),
            value: number(1.0),
        },
    ] {
        let wrong = script(vec![block(
            1,
            BlockKind::WhenStarted {
                body: vec![block(2, wrong.clone())],
            },
        )]);
        assert!(translate(&wrong).is_err(), "{wrong:?}");
    }
}

#[test]
fn the_animation_examples_blocks_drive_its_animator() {
    use axcenity_core::{KeyCode, Parameter};
    use axcenity_script::KeyEvent;
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/hello-animation/scripts/Keys.client.blocks.json");
    let text = std::fs::read_to_string(path).unwrap();
    let blocks: BlockScript = serde_json::from_str(&text).unwrap();
    let source = translate(&blocks).unwrap().source;
    for lua in [
        "workspace.Noodle.Animator:SetNumber(\"Speed\", 2)",
        "workspace.Noodle.Animator:SetTrigger(\"Hop\")",
        "workspace.Noodle.Animator:Play(\"Wave\")",
        "workspace.Noodle.Animator.StateChanged:Connect(function(state, previous)",
    ] {
        assert!(source.contains(lua), "missing `{lua}`:\n{source}");
    }

    // A world with the noodle's Animator, and the blocks as a LocalScript.
    let mut world = World::new();
    let noodle = world.create(ClassName::MeshPart).unwrap();
    world.set_name(noodle, "Noodle").unwrap();
    world.set_parent(noodle, Some(world.workspace())).unwrap();
    let animator = world.create(ClassName::Animator).unwrap();
    world.set_parent(animator, Some(noodle)).unwrap();
    let keys = world.create(ClassName::LocalScript).unwrap();
    world
        .set_parent(keys, Some(world.starter_player_scripts()))
        .unwrap();
    let mut runtime = axcenity_gameplay::script_runtime();
    runtime.start_script(&mut world, keys, "scripts/Keys.client.blocks.json", &source);
    let press = |key, pressed| KeyEvent { key, pressed };
    runtime.input(
        &mut world,
        &[press(KeyCode::W, true), press(KeyCode::Space, true)],
    );
    runtime.step(&mut world, 1.0 / 60.0);
    runtime.heartbeat(&mut world, 1.0 / 60.0);
    let live = world.animator(animator).unwrap();
    assert_eq!(live.parameters["Speed"], Parameter::Number(2.0));
    assert_eq!(live.parameters["Hop"], Parameter::Trigger(true));
    runtime.input(&mut world, &[press(KeyCode::Q, true)]);
    let request = world.animator(animator).unwrap().play.clone().unwrap();
    assert_eq!(request.state, "Wave");
    let errors: Vec<String> = runtime
        .take_output()
        .into_iter()
        .filter(|line| line.kind == OutputKind::Error)
        .map(|line| line.text)
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn the_first_saved_condition_shape_is_left_to_the_project_upgrade() {
    // Format 6 projects rewrite it as a compare block (`project`'s
    // `migrate_shapes.rs`); the reader takes only blocks.
    let saved = r#"{ "blocks": [ { "id": 1, "kind": { "type": "when_started", "body": [
        { "id": 2, "kind": { "type": "if_else",
          "condition": { "left": { "type": "number", "value": 1 }, "operator": "less",
                         "right": { "type": "variable", "name": "x" } },
          "then_body": [], "else_body": [] } } ] } } ] }"#;
    assert!(serde_json::from_str::<BlockScript>(saved).is_err());
}

#[test]
fn slots_only_take_blocks_of_their_shape() {
    assert!(!BlockShape::Statement.accepts(BlockShape::Event));
    assert!(!BlockShape::Boolean.accepts(BlockShape::Value));
    assert!(BlockShape::Value.accepts(BlockShape::Boolean));
    assert!(!BlockShape::Value.accepts(BlockShape::Statement));
    assert_eq!(
        Expression::Compare {
            op: Comparison::Equal,
            left: boxed(number(1.0)),
            right: boxed(number(1.0)),
        }
        .shape(),
        BlockShape::Boolean
    );
}

#[test]
fn mistakes_are_reported_instead_of_making_broken_lua() {
    let repeated = block(
        2,
        BlockKind::Wait {
            seconds: number(1.0),
        },
    );
    let doubled = script(vec![block(
        1,
        BlockKind::WhenStarted {
            body: vec![repeated.clone(), repeated],
        },
    )]);
    assert_eq!(translate(&doubled), Err(TranslateError::DuplicateId(2)));

    let outside = script(vec![block(
        1,
        BlockKind::WhenStarted {
            body: vec![block(
                2,
                BlockKind::Print {
                    value: Expression::TouchingPart,
                },
            )],
        },
    )]);
    assert_eq!(
        translate(&outside),
        Err(TranslateError::TouchingPartOutsideTouched)
    );

    let loose = script(vec![block(1, BlockKind::StopScript)]);
    assert_eq!(
        translate(&loose),
        Err(TranslateError::InvalidEventPlacement)
    );
}

#[test]
fn duplicates_get_new_ids_all_the_way_down() {
    let original = Block::new(BlockKind::Forever {
        body: vec![Block::new(BlockKind::StopScript)],
    });
    let copy = original.duplicate();
    let ids = |block: &Block| {
        let mut ids = vec![block.id];
        script(vec![block.clone()]).for_each_block(&mut |b| ids.push(b.id));
        ids
    };
    let (a, b) = (ids(&original), ids(&copy));
    assert!(a.iter().all(|id| !b.contains(id)), "{a:?} vs {b:?}");
}

/// Every block, as a script: one of each event, each holding every statement.
fn every_block_script() -> BlockScript {
    let mut id = 100;
    let mut body = || {
        every_statement()
            .into_iter()
            // These end the Lua block they're in, so they can't go in a row.
            .filter(|(kind, _)| {
                !matches!(
                    kind,
                    BlockKind::Code { .. }
                        | BlockKind::Break
                        | BlockKind::Continue
                        | BlockKind::Return { .. }
                )
            })
            .map(|(kind, _)| {
                id += 1;
                block(id, kind)
            })
            .collect::<Vec<_>>()
    };
    let mut touched_body = body();
    touched_body.push(block(
        90,
        BlockKind::Print {
            value: Expression::TouchingPart,
        },
    ));
    let stacks = [
        block(1, BlockKind::WhenStarted { body: body() }),
        block(
            2,
            BlockKind::WhenTouched {
                part: "Pad".to_owned(),
                body: touched_body,
            },
        ),
        block(
            3,
            BlockKind::WhenKeyPressed {
                key: "Space".to_owned(),
                body: body(),
            },
        ),
        block(
            4,
            BlockKind::WhenClicked {
                button: "Menu.Start".to_owned(),
                body: body(),
            },
        ),
        block(5, BlockKind::EveryFrame { body: body() }),
        block(
            6,
            BlockKind::WhenSoundEnded {
                sound: "Music".to_owned(),
                body: body(),
            },
        ),
        block(
            7,
            BlockKind::WhenStateChanged {
                animator: animator(),
                body: body(),
            },
        ),
        block(
            8,
            BlockKind::WhenAnimationEvent {
                animator: animator(),
                event: "Footstep".to_owned(),
                body: body(),
            },
        ),
        block(
            9,
            BlockKind::WhenMouseEnters {
                element: "Menu.Start".to_owned(),
                body: body(),
            },
        ),
        block(
            10,
            BlockKind::WhenMouseLeaves {
                element: "Menu.Start".to_owned(),
                body: body(),
            },
        ),
        block(
            11,
            BlockKind::WhenValueChanged {
                widget: "Settings.Volume".to_owned(),
                body: body(),
            },
        ),
        block(
            12,
            BlockKind::WhenAgentSenses {
                agent: "Guard.NavigationAgent".to_owned(),
                sense: "Hearing".to_owned(),
                body: body(),
            },
        ),
        block(
            16,
            BlockKind::WhenSeatOccupied {
                seat: "Cart.Seat".to_owned(),
                body: body(),
            },
        ),
        block(
            17,
            BlockKind::WhenPromptTriggered {
                prompt: "Door.ProximityPrompt".to_owned(),
                body: body(),
            },
        ),
        block(
            18,
            BlockKind::WhenPartClicked {
                detector: "Button.ClickDetector".to_owned(),
                body: body(),
            },
        ),
        block(
            21,
            BlockKind::WhenCommandRun {
                command: text("heal"),
                help: text("Heals the player"),
                body: body(),
            },
        ),
        block(13, BlockKind::BeforePhysics { body: body() }),
        block(14, BlockKind::BeforeRender { body: body() }),
        block(
            15,
            BlockKind::WhenEventFired {
                event: "Events.Scored".to_owned(),
                body: body(),
            },
        ),
        block(
            19,
            BlockKind::WhenCutsceneFinished {
                sequence: "Intro".to_owned(),
                body: body(),
            },
        ),
        block(20, BlockKind::WhenConnectionFails { body: body() }),
        block(
            21,
            BlockKind::WhenSplineReached {
                follower: "Lift.SplineFollower".to_owned(),
                which: "Start".to_owned(),
                body: body(),
            },
        ),
    ];
    // New IDs all the way down, as the statements are copies.
    script(stacks.iter().map(Block::duplicate).collect())
}

/// Whether any of `script` came back as grey Lua blocks.
fn has_code(script: &BlockScript) -> bool {
    let mut found = false;
    script.for_each_block(&mut |block| {
        found |= matches!(
            block.kind,
            BlockKind::Code { .. } | BlockKind::TopCode { .. }
        ) || block
            .kind
            .expressions()
            .iter()
            .any(|e| matches!(e, Expression::Code { .. }));
    });
    found
}

#[test]
fn blocks_switched_to_code_and_back_are_the_same_blocks() {
    let original = every_block_script();
    let lua = translate(&original).unwrap().source;
    let back = from_lua(&lua).unwrap();
    assert!(!has_code(&back), "every block should come back:\n{back:#?}");
    assert_eq!(translate(&back).unwrap().source, lua);
}

#[test]
fn reporters_switched_to_code_and_back_are_the_same_reporters() {
    let every_reporter = vec![
        Expression::Math {
            op: MathOp::Power,
            left: boxed(variable("a")),
            right: boxed(number(-2.0)),
        },
        Expression::Function {
            function: MathFunction::Round,
            value: boxed(number(2.5)),
        },
        Expression::Function {
            function: MathFunction::Sqrt,
            value: boxed(number(9.0)),
        },
        Expression::Random {
            min: boxed(number(1.0)),
            max: boxed(number(6.0)),
        },
        Expression::RandomNumber {
            min: boxed(number(-1.0)),
            max: boxed(variable("a")),
        },
        Expression::Lerp {
            from: boxed(Expression::Property {
                target: "Pad".to_owned(),
                property: "Position".to_owned(),
            }),
            to: boxed(Expression::Vector3 {
                x: 0.0,
                y: 10.0,
                z: 0.0,
            }),
            alpha: boxed(number(0.5)),
        },
        Expression::Join {
            left: boxed(Expression::Text {
                value: "a \"quoted\"\n".to_owned(),
            }),
            right: boxed(variable("a")),
        },
        Expression::Logic {
            op: LogicOp::Or,
            left: boxed(Expression::Not {
                value: boxed(Expression::Boolean { value: false }),
            }),
            right: boxed(Expression::Compare {
                op: Comparison::GreaterOrEqual,
                left: boxed(Expression::Property {
                    target: "Pad".to_owned(),
                    property: "Transparency".to_owned(),
                }),
                right: boxed(number(0.5)),
            }),
        },
        Expression::KeyDown {
            key: "W".to_owned(),
        },
        Expression::Color3 {
            r: 0.0,
            g: 0.5,
            b: 1.0,
        },
        Expression::AnimatorBool {
            animator: animator(),
            parameter: "IsGrounded".to_owned(),
        },
        Expression::AnimatorNumber {
            animator: animator(),
            parameter: "Speed".to_owned(),
        },
        Expression::AnimatorLayerWeight {
            animator: animator(),
            layer: "UpperBody".to_owned(),
        },
        Expression::Property {
            target: animator(),
            property: "CurrentState".to_owned(),
        },
        Expression::IsA {
            value: boxed(variable("a")),
            class: "BasePart".to_owned(),
        },
        Expression::Raycast {
            origin: boxed(Expression::Vector3 {
                x: 0.0,
                y: 10.0,
                z: 0.0,
            }),
            direction: boxed(Expression::Vector3 {
                x: 0.0,
                y: -20.0,
                z: 0.0,
            }),
        },
        Expression::RaycastIgnoring {
            origin: boxed(variable("a")),
            direction: boxed(Expression::Vector3 {
                x: 0.0,
                y: 0.0,
                z: -50.0,
            }),
            ignore: "Shooter".to_owned(),
        },
        Expression::PartUnderMouse,
        Expression::DataRow {
            table: "weapons".to_owned(),
            key: boxed(Expression::Text {
                value: "sword".to_owned(),
            }),
        },
        Expression::DataValue {
            column: "Damage".to_owned(),
            row: boxed(Expression::DataRow {
                table: "weapons".to_owned(),
                key: boxed(variable("a")),
            }),
        },
        Expression::SplinePosition {
            spline: "Track".to_owned(),
            t: boxed(number(0.5)),
        },
        Expression::SplineClosestDistance {
            spline: "Track".to_owned(),
            position: boxed(variable("a")),
        },
        Expression::DataValue {
            column: "Fire Rate".to_owned(),
            row: boxed(Expression::DataRow {
                table: "enemies/bosses".to_owned(),
                key: boxed(Expression::Text {
                    value: "boss".to_owned(),
                }),
            }),
        },
    ];
    let body = every_reporter
        .into_iter()
        .enumerate()
        .map(|(index, value)| block(10 + index as u64, BlockKind::Print { value }))
        .collect();
    let original = script(vec![block(1, BlockKind::EveryFrame { body })]);
    let lua = translate(&original).unwrap().source;
    let back = from_lua(&lua).unwrap();
    assert!(
        !has_code(&back),
        "every reporter should come back:\n{back:#?}"
    );
    assert_eq!(translate(&back).unwrap().source, lua);
}

#[test]
fn instance_tree_blocks_give_the_same_lua_as_the_typed_calls() {
    // As "when game starts" writes it.
    let typed = "\
task.spawn(function()
    workspace.Coin:Clone().Parent = workspace.Bank
    for _, child in workspace.Bank:GetChildren() do
        print(child:IsA(\"BasePart\"), child:IsA(\"Model\"))
    end
    print(workspace.Bank:WaitForChild(\"Coin\"))
    workspace.Coin:Destroy()
end)
";
    let blocks = from_lua(typed).unwrap();
    let [
        Block {
            kind: BlockKind::WhenStarted { body },
            ..
        },
    ] = blocks.blocks.as_slice()
    else {
        panic!("unexpected blocks: {:#?}", blocks.blocks);
    };
    let kinds: Vec<_> = body.iter().map(|block| &block.kind).collect();
    assert!(
        matches!(
            kinds.as_slice(),
            [
                BlockKind::CloneInto { .. },
                BlockKind::ForEachChild { .. },
                BlockKind::Print {
                    value: Expression::MethodCall { .. }
                },
                BlockKind::Destroy { .. },
            ]
        ),
        "{kinds:#?}"
    );
    // Blocks made from the typed calls give back exactly that code.
    let source = translate(&blocks).unwrap().source;
    assert_eq!(source, typed);

    // The same blocks built by hand give the same Lua too.
    let is_a = |class: &str| Expression::IsA {
        value: boxed(variable("child")),
        class: class.to_owned(),
    };
    let by_hand = script(vec![block(
        1,
        BlockKind::WhenStarted {
            body: vec![
                block(
                    2,
                    BlockKind::CloneInto {
                        target: "Coin".to_owned(),
                        parent: "Bank".to_owned(),
                    },
                ),
                block(
                    3,
                    BlockKind::ForEachChild {
                        target: "Bank".to_owned(),
                        variable: "child".to_owned(),
                        body: vec![block(
                            4,
                            BlockKind::Print {
                                value: is_a("Part"),
                            },
                        )],
                    },
                ),
                block(
                    5,
                    BlockKind::WaitForChild {
                        target: "Bank".to_owned(),
                        name: "Coin".to_owned(),
                    },
                ),
            ],
        },
    )]);
    let hand = translate(&by_hand).unwrap().source;
    assert!(
        hand.starts_with(
            "task.spawn(function()\n    \
             workspace.Coin:Clone().Parent = workspace.Bank\n    \
             for _, child in workspace.Bank:GetChildren() do\n"
        ),
        "{hand}"
    );
    assert!(
        hand.contains(
            "\n        print(child:IsA(\"Part\"))\n    end\n    workspace.Bank:WaitForChild(\"Coin\")\n"
        ),
        "{hand}"
    );
    assert!(!hand.contains("local child"), "{hand}");

    // And it runs.
    let mut world = World::new();
    let bank = world.create(ClassName::Folder).unwrap();
    world.set_name(bank, "Bank").unwrap();
    world.set_parent(bank, Some(world.workspace())).unwrap();
    let coin = world.create(ClassName::Part).unwrap();
    world.set_name(coin, "Coin").unwrap();
    world.set_parent(coin, Some(world.workspace())).unwrap();
    let lua = world.create(ClassName::Script).unwrap();
    world
        .set_parent(lua, Some(world.server_script_service()))
        .unwrap();
    let mut runtime = axcenity_gameplay::script_runtime();
    runtime.start_script(&mut world, lua, "scripts/Bank.server.blocks.json", &source);
    runtime.step(&mut world, 0.1);
    let output: Vec<_> = runtime
        .take_output()
        .into_iter()
        .map(|line| line.text)
        .collect();
    assert_eq!(output, ["true\tfalse", "Coin"]);
    assert!(world.find_first_child(world.workspace(), "Coin").is_none());
    assert_eq!(world.get(bank).unwrap().children().len(), 1);
}

#[test]
fn typed_lua_becomes_blocks_in_the_order_it_was_typed() {
    let typed = "\
local count = 0
-- Doubles a number.
local function double(x)
    return x * 2
end

workspace.Pad.Touched:Connect(function(hit)
    count = count + 1
    for i = 1, 2 do
        print(i)
    end
    print(double(count))
end)
print(\"Hello world!\")
";
    let blocks = from_lua(typed).unwrap();
    let kinds: Vec<_> = blocks.blocks.iter().map(|block| &block.kind).collect();
    // `hit` isn't the name "when touched" gives the part, so it's a signal.
    let [
        BlockKind::RunScript { body: first },
        BlockKind::OnSignal { params, body, .. },
        BlockKind::RunScript { body: last },
    ] = kinds.as_slice()
    else {
        panic!("unexpected blocks: {kinds:#?}");
    };
    assert_eq!(
        first[0].kind,
        BlockKind::Comment {
            text: "-- Doubles a number.".to_owned()
        }
    );
    assert!(matches!(
        &first[1].kind,
        BlockKind::DefineFunction { name, local: true, params, .. } if name == "double" && params == "x"
    ));
    assert_eq!(params, "hit");
    assert!(matches!(&body[0].kind, BlockKind::ChangeVariable { name, .. } if name == "count"));
    assert!(matches!(&body[1].kind, BlockKind::NumericFor { variable, .. } if variable == "i"));
    assert!(matches!(
        &body[2].kind,
        BlockKind::Print {
            value: Expression::Call { .. }
        }
    ));
    assert!(matches!(&last[0].kind, BlockKind::Print { .. }));

    // And it still runs.
    let mut world = World::new();
    let pad = world.create(ClassName::Part).unwrap();
    world.set_name(pad, "Pad").unwrap();
    world.set_parent(pad, Some(world.workspace())).unwrap();
    let lua = world.create(ClassName::Script).unwrap();
    world
        .set_parent(lua, Some(world.server_script_service()))
        .unwrap();
    let source = translate(&blocks).unwrap().source;
    let mut runtime = axcenity_gameplay::script_runtime();
    runtime.start_script(&mut world, lua, "scripts/Typed.server.lua", &source);
    runtime.step(&mut world, 0.1);
    let output: Vec<_> = runtime
        .take_output()
        .into_iter()
        .map(|line| line.text)
        .collect();
    assert_eq!(output, ["Hello world!"], "{source}");
}

#[test]
fn luau_syntax_becomes_blocks_or_stays_as_lua() {
    let typed = "\
local count = 0
type Point = { x: number, y: number }
local function double(x: number): number
    return x * 2
end

count += 2
for i = 1, 3 do
    if i == 2 then continue end
    print(`step {i} of {count}`)
end
local label: string = if count > 1 then \"many\" else \"one\"
print(label, double(count :: number), 0b101 + 1_000)
";
    let blocks = from_lua(typed).unwrap();
    let kinds: Vec<_> = blocks.blocks.iter().map(|block| &block.kind).collect();
    let [BlockKind::RunScript { body }] = kinds.as_slice() else {
        panic!("unexpected blocks: {kinds:#?}");
    };
    // A type definition does nothing when run, so it's the only grey block.
    assert!(matches!(&body[0].kind, BlockKind::Code { code } if code.starts_with("type Point = ")));
    assert!(matches!(
        &body[1].kind,
        BlockKind::DefineFunction { returns, params, .. } if returns == ": number" && params == "x: number"
    ));
    assert!(matches!(&body[2].kind, BlockKind::CompoundAssign { op, .. } if op == "+"));
    assert!(matches!(&body[3].kind, BlockKind::NumericFor { .. }));
    assert!(matches!(&body[4].kind, BlockKind::Local { names, .. } if names == "label: string"));
    assert!(matches!(
        &body[5].kind,
        BlockKind::CallFunction { .. } | BlockKind::Print { .. }
    ));

    // Switching again changes nothing more.
    let source = translate(&blocks).unwrap().source;
    assert_eq!(
        translate(&from_lua(&source).unwrap()).unwrap().source,
        source
    );

    // And it still runs.
    let mut world = World::new();
    let lua = world.create(ClassName::Script).unwrap();
    world
        .set_parent(lua, Some(world.server_script_service()))
        .unwrap();
    let mut runtime = axcenity_gameplay::script_runtime();
    runtime.start_script(&mut world, lua, "scripts/Luau.server.lua", &source);
    runtime.step(&mut world, 0.1);
    let output: Vec<_> = runtime
        .take_output()
        .into_iter()
        .map(|line| line.text)
        .collect();
    assert_eq!(
        output,
        ["step 1 of 2", "step 3 of 2", "many\t4\t1005"],
        "{source}"
    );

    // A handler with type annotations keeps them.
    let typed = "workspace.Pad.Touched:Connect(function(hit: BasePart)\n    print(hit)\nend)\n";
    let blocks = from_lua(typed).unwrap();
    assert!(
        matches!(&blocks.blocks[..], [Block { kind: BlockKind::OnSignal { params, .. }, .. }] if params == "hit: BasePart"),
        "{blocks:#?}"
    );
    assert_eq!(translate(&blocks).unwrap().source, typed);
    // `+=` keeps its own block: "move by" would write it another way.
    let blocks = from_lua(
        "workspace.Pad.Touched:Connect(function()\n    workspace.Pad.Position += Vector3.new(0, 1, 0)\nend)\n",
    )
    .unwrap();
    let [
        Block {
            kind: BlockKind::OnSignal { body, .. },
            ..
        },
    ] = &blocks.blocks[..]
    else {
        panic!("{blocks:#?}");
    };
    assert!(matches!(&body[0].kind, BlockKind::CompoundAssign { op, .. } if op == "+"));
}

#[test]
fn lua_5_4_syntax_that_luau_lacks_is_a_syntax_error() {
    for code in [
        "goto done",
        "local x <const> = 1",
        "print(1 << 2)",
        "::done::",
    ] {
        assert!(from_lua(code).is_err(), "{code}");
    }
    // `goto`, `continue` and `type` are names in Luau.
    assert!(from_lua("local goto = 1\nlocal continue = 2\nprint(type(goto), continue)").is_ok());
}

#[test]
fn top_level_code_that_waits_stays_in_order() {
    // Statements stay where they were typed, with nothing around them, so
    // the wait is over before the event is connected.
    let typed = "task.wait(2)\nworkspace.Pad.Touched:Connect(function()\nend)\n";
    let blocks = from_lua(typed).unwrap();
    let [
        Block {
            kind: BlockKind::RunScript { body },
            ..
        },
        Block {
            kind: BlockKind::OnSignal { .. },
            ..
        },
    ] = blocks.blocks.as_slice()
    else {
        panic!("{blocks:#?}");
    };
    assert!(matches!(body[0].kind, BlockKind::Wait { .. }));
    assert_eq!(translate(&blocks).unwrap().source, typed);
    // A new script's code is a "script runs" stack, and writes the same.
    let typed = "print(\"Hello world!\")\n";
    let blocks = from_lua(typed).unwrap();
    assert!(matches!(
        &blocks.blocks[..],
        [Block {
            kind: BlockKind::RunScript { .. },
            ..
        }]
    ));
    assert_eq!(translate(&blocks).unwrap().source, typed);
}

#[test]
fn code_with_a_syntax_error_says_where() {
    let err = from_lua("print(\"hi\")\nif x then\n").unwrap_err();
    let FromLuaError::Syntax(err) = err else {
        panic!("{err:?}");
    };
    assert_eq!(err.line, 3);
    assert!(from_lua("x = = 1").is_err());
}

#[test]
fn every_example_script_can_switch_to_blocks_and_back() {
    let examples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut checked = 0;
    for project in std::fs::read_dir(examples).unwrap() {
        let scripts = project.unwrap().path().join("scripts");
        let Ok(files) = std::fs::read_dir(scripts) else {
            continue;
        };
        for file in files {
            let path = file.unwrap().path();
            let name = path.to_string_lossy();
            if !(name.ends_with(".server.lua") || name.ends_with(".client.lua")) {
                continue;
            }
            let source = std::fs::read_to_string(&path).unwrap();
            let blocks = from_lua(&source).unwrap_or_else(|err| panic!("{name}: {err}"));
            // Switching again changes nothing more.
            let lua = translate(&blocks).unwrap().source;
            let again = translate(&from_lua(&lua).unwrap()).unwrap().source;
            assert_eq!(lua, again, "{name}");
            checked += 1;
        }
    }
    assert!(checked > 5, "only {checked} example scripts found");
}

#[test]
fn the_graph_view_is_saved_only_once_used_and_never_changes_the_lua() {
    let plain = script(vec![block(
        1,
        BlockKind::WhenStarted {
            body: vec![block(2, BlockKind::Print { value: number(1.0) })],
        },
    )]);
    let text = serde_json::to_string(&plain).unwrap();
    assert!(!text.contains("graph"), "{text}");
    assert_eq!(serde_json::from_str::<BlockScript>(&text).unwrap(), plain);

    let mut shown = plain.clone();
    shown.graph.shown = true;
    shown.graph.nodes.insert("2".to_owned(), [100.0, 40.0]);
    shown.graph.loose.push(LooseValue {
        id: 7,
        value: variable("score"),
    });
    let text = serde_json::to_string(&shown).unwrap();
    assert_eq!(serde_json::from_str::<BlockScript>(&text).unwrap(), shown);
    assert_eq!(
        translate(&shown).unwrap().source,
        translate(&plain).unwrap().source
    );
    // New blocks never take a loose value node's id.
    reserve_block_ids(&shown);
    assert!(Block::new(BlockKind::StopScript).id > 7);
}

#[test]
fn set_property_blocks_turn_a_frame_and_fade_a_labels_text() {
    let mut world = World::new();
    let gui = world.create(ClassName::ScreenGui).unwrap();
    world.set_name(gui, "Hud").unwrap();
    world.set_parent(gui, Some(world.workspace())).unwrap();
    let panel = world.create(ClassName::Frame).unwrap();
    world.set_name(panel, "Panel").unwrap();
    world.set_parent(panel, Some(gui)).unwrap();
    let title = world.create(ClassName::TextLabel).unwrap();
    world.set_name(title, "Title").unwrap();
    world.set_parent(title, Some(panel)).unwrap();

    let set = |id, target: &str, property: &str, value: f64| {
        block(
            id,
            BlockKind::SetProperty {
                target: target.to_owned(),
                property: property.to_owned(),
                value: number(value),
            },
        )
    };
    let blocks = vec![block(
        1,
        BlockKind::WhenStarted {
            body: vec![
                set(2, "Hud.Panel", "Rotation", 30.0),
                set(3, "Hud.Panel.Title", "TextTransparency", 0.5),
            ],
        },
    )];
    let source = translate(&script(blocks)).unwrap().source;
    assert!(
        source.contains("workspace.Hud.Panel.Rotation = 30"),
        "{source}"
    );
    assert!(
        source.contains("workspace.Hud.Panel.Title.TextTransparency = 0.5"),
        "{source}"
    );

    let lua = world.create(ClassName::LocalScript).unwrap();
    world.set_name(lua, "Look").unwrap();
    world.set_parent(lua, Some(gui)).unwrap();
    let mut runtime = axcenity_gameplay::script_runtime();
    runtime.start_script(&mut world, lua, "scripts/Look.client.blocks.json", &source);
    runtime.step(&mut world, 0.1);
    let number = |id, name| world.get(id).unwrap().property(name).unwrap().clone();
    assert_eq!(number(panel, "Rotation"), Value::Number(30.0));
    assert_eq!(number(title, "TextTransparency"), Value::Number(0.5));
}

#[test]
fn look_at_move_forward_lerp_and_random_number_blocks_run_and_come_back_from_lua() {
    let goal = || Expression::Property {
        target: "Goal".to_owned(),
        property: "Position".to_owned(),
    };
    let body = vec![
        block(
            2,
            BlockKind::LookAt {
                target: "Pad".to_owned(),
                position: goal(),
            },
        ),
        block(
            3,
            BlockKind::MoveForward {
                target: "Pad".to_owned(),
                distance: number(5.0),
            },
        ),
        block(
            4,
            BlockKind::Print {
                value: Expression::Lerp {
                    from: boxed(Expression::Vector3 {
                        x: 0.0,
                        y: 0.0,
                        z: 0.0,
                    }),
                    to: boxed(goal()),
                    alpha: boxed(number(0.5)),
                },
            },
        ),
        block(
            5,
            BlockKind::SetVariable {
                name: "roll".to_owned(),
                value: Expression::RandomNumber {
                    min: boxed(number(2.0)),
                    max: boxed(number(3.0)),
                },
            },
        ),
        block(
            6,
            BlockKind::Print {
                value: Expression::Logic {
                    op: LogicOp::And,
                    left: boxed(Expression::Compare {
                        op: Comparison::GreaterOrEqual,
                        left: boxed(variable("roll")),
                        right: boxed(number(2.0)),
                    }),
                    right: boxed(Expression::Compare {
                        op: Comparison::Less,
                        left: boxed(variable("roll")),
                        right: boxed(number(3.0)),
                    }),
                },
            },
        ),
    ];
    let blocks = script(vec![block(1, BlockKind::WhenStarted { body })]);
    let lua = translate(&blocks).unwrap().source;
    let back = from_lua(&lua).unwrap();
    assert!(!has_code(&back), "{back:#?}");
    assert_eq!(translate(&back).unwrap().source, lua);

    let mut world = World::new();
    let workspace = world.workspace();
    for (name, position) in [
        ("Pad", Vector3::new(0.0, 0.0, 0.0)),
        ("Goal", Vector3::new(10.0, 0.0, 0.0)),
    ] {
        let part = world.create(ClassName::Part).unwrap();
        world.set_name(part, name).unwrap();
        world
            .set_property(part, "Position", Value::Vector3(position))
            .unwrap();
        world.set_parent(part, Some(workspace)).unwrap();
    }
    let script_id = world.create(ClassName::Script).unwrap();
    world
        .set_parent(script_id, Some(world.server_script_service()))
        .unwrap();
    let mut runtime = axcenity_gameplay::script_runtime();
    runtime.start_script(&mut world, script_id, "Look.server.blocks.json", &lua);
    runtime.step(&mut world, 0.0);
    let output: Vec<String> = runtime
        .take_output()
        .into_iter()
        .map(|line| match line.kind {
            OutputKind::Error => format!("error: {}", line.text),
            _ => line.text,
        })
        .collect();
    assert_eq!(output, ["5, 0, 0", "true"]);
    // Facing +X (turned right, -90 around Y), then 5 studs that way.
    let pad = world.find_first_child(workspace, "Pad").unwrap();
    let pad = world.get(pad).unwrap();
    assert_eq!(
        pad.property("Orientation"),
        Some(&Value::Vector3(Vector3::new(0.0, -90.0, 0.0)))
    );
    match pad.property("Position") {
        Some(Value::Vector3(v)) => {
            assert!(
                (v.x - 5.0).abs() < 1e-5 && v.y.abs() < 1e-5 && v.z.abs() < 1e-5,
                "{v:?}"
            )
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn camera_blocks_hold_point_and_shake_the_camera_and_find_the_part_under_the_mouse() {
    use axcenity_core::input::{InputEvent, InputKind};
    use axcenity_script::CameraState;

    let source = translate(&script(vec![block(
        1,
        BlockKind::WhenStarted {
            body: vec![
                block(
                    2,
                    BlockKind::SetCameraType {
                        camera_type: "Scriptable".to_owned(),
                    },
                ),
                block(
                    3,
                    BlockKind::PointCameraAt {
                        position: Expression::Vector3 {
                            x: 0.0,
                            y: 0.0,
                            z: 0.0,
                        },
                    },
                ),
                block(
                    4,
                    BlockKind::ShakeCamera {
                        intensity: number(1.0),
                        seconds: number(0.5),
                    },
                ),
                block(
                    5,
                    BlockKind::Wait {
                        seconds: number(0.0),
                    },
                ),
                block(
                    6,
                    BlockKind::Print {
                        value: Expression::PartUnderMouse,
                    },
                ),
            ],
        },
    )]))
    .unwrap()
    .source;
    let mut world = World::new();
    let pad = world.create(ClassName::Part).unwrap();
    world.set_name(pad, "Pad").unwrap();
    world.set_parent(pad, Some(world.workspace())).unwrap();
    let mut runtime = axcenity_gameplay::script_runtime();
    runtime.set_viewport_size(200.0, 100.0);
    runtime.set_camera(CameraState {
        position: Vector3::new(0.0, 6.0, 8.0),
        orientation: Vector3::new(0.0, 0.0, 0.0),
    });
    let local = world.create(ClassName::LocalScript).unwrap();
    world
        .set_parent(local, Some(world.starter_player_scripts()))
        .unwrap();
    runtime.start_script(&mut world, local, "scripts/Camera.client.lua", &source);
    // The camera now looks at the pad, so the middle of the view is on it.
    let mouse = InputKind::MouseMove {
        position: [100.0, 50.0],
        delta: [0.0, 0.0],
    };
    runtime.input_events(&mut world, &[InputEvent::new(mouse)]);
    runtime.step(&mut world, 1.0 / 60.0);
    let output: Vec<_> = runtime
        .take_output()
        .into_iter()
        .map(|line| line.text)
        .collect();
    assert_eq!(output, ["Pad"]);
    let game = runtime.step_camera(&world, 1.0 / 60.0);
    let held = game.scripted.expect("Scriptable");
    assert_eq!(held.position, Vector3::new(0.0, 6.0, 8.0));
    assert!(
        held.orientation.x < -30.0,
        "looks down at the pad: {held:?}"
    );
    assert_ne!(game.shake.position, Vector3::default());
}

#[test]
fn event_and_frame_blocks_fire_bindables_destroy_later_and_run_before_physics_and_render() {
    let server = translate(&script(vec![
        block(
            1,
            BlockKind::WhenEventFired {
                event: "Scored".to_owned(),
                body: vec![
                    block(
                        2,
                        BlockKind::Print {
                            value: Expression::Text {
                                value: "scored".to_owned(),
                            },
                        },
                    ),
                    block(
                        3,
                        BlockKind::DestroyAfter {
                            target: "Spark".to_owned(),
                            seconds: number(0.5),
                        },
                    ),
                ],
            },
        ),
        block(
            4,
            BlockKind::BeforePhysics {
                body: vec![block(
                    5,
                    BlockKind::ChangeVariable {
                        name: "steps".to_owned(),
                        by: number(1.0),
                    },
                )],
            },
        ),
        block(
            6,
            BlockKind::WhenStarted {
                body: vec![block(
                    7,
                    BlockKind::FireEvent {
                        event: "Scored".to_owned(),
                    },
                )],
            },
        ),
    ]))
    .unwrap()
    .source;
    let client = translate(&script(vec![block(
        1,
        BlockKind::BeforeRender {
            body: vec![block(
                2,
                BlockKind::Print {
                    value: Expression::Text {
                        value: "render".to_owned(),
                    },
                },
            )],
        },
    )]))
    .unwrap()
    .source;
    // Both come back from Lua as the same blocks.
    for lua in [&server, &client] {
        let back = from_lua(lua).unwrap();
        assert!(!has_code(&back), "{back:#?}");
        assert_eq!(&translate(&back).unwrap().source, lua);
    }

    let mut world = World::new();
    let scored = world.create(ClassName::BindableEvent).unwrap();
    world.set_name(scored, "Scored").unwrap();
    world
        .set_parent(scored, Some(world.replicated_storage()))
        .unwrap();
    let spark = world.create(ClassName::Part).unwrap();
    world.set_name(spark, "Spark").unwrap();
    world.set_parent(spark, Some(world.workspace())).unwrap();
    let mut runtime = axcenity_gameplay::script_runtime();
    let script = world.create(ClassName::Script).unwrap();
    world
        .set_parent(script, Some(world.server_script_service()))
        .unwrap();
    runtime.start_script(&mut world, script, "scripts/Blocks.server.lua", &server);
    let local = world.create(ClassName::LocalScript).unwrap();
    world
        .set_parent(local, Some(world.starter_player_scripts()))
        .unwrap();
    runtime.start_script(&mut world, local, "scripts/Blocks.client.lua", &client);
    let said = |runtime: &mut ScriptRuntime| -> Vec<String> {
        runtime
            .take_output()
            .into_iter()
            .map(|line| {
                assert_eq!(line.kind, OutputKind::Print, "{}", line.text);
                line.text
            })
            .collect()
    };
    assert_eq!(said(&mut runtime), ["scored"]);
    for _ in 0..30 {
        runtime.step(&mut world, 1.0 / 60.0);
        runtime.stepped(&mut world, 1.0 / 60.0);
        runtime.render_step(&mut world, 1.0 / 60.0);
    }
    assert!(!world.contains(spark), "destroyed half a second later");
    assert_eq!(said(&mut runtime), vec!["render"; 30]);
    runtime.run_command(&mut world, "print('ok')").unwrap();
    assert_eq!(said(&mut runtime), ["ok"]);
}

/// Phase 38: "for each part touching", "for each part within" and "raycast
/// … ignoring …" run as the queries they translate to.
#[test]
fn query_blocks_list_the_parts_in_a_zone_and_raycast_past_the_shooter() {
    let mut world = World::new();
    let mut add = |name: &str, position: Vector3, size: Vector3| {
        let id = world.create(ClassName::Part).unwrap();
        world.set_name(id, name).unwrap();
        world.set_parent(id, Some(world.workspace())).unwrap();
        for (property, value) in [
            ("Position", Value::Vector3(position)),
            ("Size", Value::Vector3(size)),
            ("Anchored", Value::Bool(true)),
        ] {
            world.set_property(id, property, value).unwrap();
        }
    };
    add(
        "Zone",
        Vector3::new(0.0, 5.0, 0.0),
        Vector3::new(10.0, 10.0, 10.0),
    );
    add(
        "Coin",
        Vector3::new(2.0, 2.0, 2.0),
        Vector3::new(1.0, 1.0, 1.0),
    );
    add(
        "Shooter",
        Vector3::new(0.0, 5.0, -10.0),
        Vector3::new(2.0, 2.0, 2.0),
    );
    add(
        "Wall",
        Vector3::new(0.0, 5.0, -20.0),
        Vector3::new(10.0, 10.0, 1.0),
    );

    let print_name = |id: u64, of: &str| {
        block(
            id,
            BlockKind::Print {
                value: Expression::Code {
                    code: format!("{of}.Name"),
                },
            },
        )
    };
    let along = |z: f64| Expression::Vector3 { x: 0.0, y: 0.0, z };
    let from = || Expression::Vector3 {
        x: 0.0,
        y: 5.0,
        z: -6.0,
    };
    let blocks = vec![block(
        1,
        BlockKind::WhenStarted {
            body: vec![
                block(
                    2,
                    BlockKind::ForEachPartTouching {
                        target: "Zone".to_owned(),
                        variable: "inside".to_owned(),
                        body: vec![print_name(3, "inside")],
                    },
                ),
                block(
                    4,
                    BlockKind::ForEachPartInRadius {
                        center: Expression::Vector3 {
                            x: 2.0,
                            y: 2.0,
                            z: 2.0,
                        },
                        radius: number(0.5),
                        variable: "near".to_owned(),
                        body: vec![print_name(5, "near")],
                    },
                ),
                block(
                    6,
                    BlockKind::SetVariable {
                        name: "hit".to_owned(),
                        value: Expression::RaycastIgnoring {
                            origin: boxed(from()),
                            direction: boxed(along(-30.0)),
                            ignore: "Shooter".to_owned(),
                        },
                    },
                ),
                print_name(7, "hit.Instance"),
                block(
                    8,
                    BlockKind::SetVariable {
                        name: "hit".to_owned(),
                        value: Expression::Raycast {
                            origin: boxed(from()),
                            direction: boxed(along(-30.0)),
                        },
                    },
                ),
                print_name(9, "hit.Instance"),
            ],
        },
    )];
    let source = translate(&script(blocks)).unwrap().source;
    let lua = world.create(ClassName::Script).unwrap();
    world
        .set_parent(lua, Some(world.server_script_service()))
        .unwrap();
    let mut runtime = axcenity_gameplay::script_runtime();
    runtime.start_script(&mut world, lua, "scripts/Q.server.blocks.json", &source);
    let output: Vec<_> = runtime
        .take_output()
        .into_iter()
        .map(|line| line.text)
        .collect();
    assert_eq!(
        output,
        ["Coin", "Zone", "Coin", "Wall", "Shooter"],
        "{source}"
    );
}

/// The Spawn Decal block places a decal on what's at its position, facing
/// the way given, which fades and goes after its seconds (Phase 75).
#[test]
fn the_spawn_decal_block_places_a_decal_that_goes_after_its_seconds() {
    let up = || Expression::Vector3 {
        x: 0.0,
        y: 1.0,
        z: 0.0,
    };
    let source = translate(&script(vec![block(
        1,
        BlockKind::WhenStarted {
            body: vec![block(
                2,
                BlockKind::SpawnDecal {
                    texture: text("assets/textures/hole.png"),
                    position: Expression::Vector3 {
                        x: 2.0,
                        y: 0.0,
                        z: 3.0,
                    },
                    facing: up(),
                    size: number(0.5),
                    seconds: number(2.0),
                },
            )],
        },
    )]))
    .unwrap()
    .source;
    let mut world = World::new();
    let lua = world.create(ClassName::Script).unwrap();
    world
        .set_parent(lua, Some(world.server_script_service()))
        .unwrap();
    let mut runtime = axcenity_gameplay::script_runtime();
    runtime.start_script(&mut world, lua, "scripts/Holes.server.blocks.json", &source);
    let decal = world
        .descendants(world.workspace())
        .into_iter()
        .find(|&id| world.get(id).unwrap().class() == ClassName::ProjectedDecal)
        .expect("a decal");
    let property = |world: &World, name| world.get(decal).unwrap().property(name).cloned();
    assert_eq!(
        property(&world, "Position"),
        Some(Value::Vector3(Vector3::new(2.0, 0.0, 3.0)))
    );
    assert_eq!(
        property(&world, "Size"),
        Some(Value::Vector3(Vector3::new(0.5, 0.5, 0.5)))
    );
    // Facing up: turned 90 degrees around X.
    let Some(Value::Vector3(orientation)) = property(&world, "Orientation") else {
        panic!("an orientation");
    };
    assert!((orientation.x - 90.0).abs() < 1e-3, "{orientation:?}");
    assert!(runtime.take_output().is_empty());
    for _ in 0..3 {
        runtime.step(&mut world, 1.0);
    }
    assert!(!world.contains(decal), "gone after 2 seconds");
}

#[test]
fn streaming_blocks_become_lua_and_come_back() {
    use crate::{BlockKind, Expression};

    let source = "\
workspace.Castle.StreamedIn:Connect(function()
    print(workspace.Castle.StreamingState == Enum.StreamingState.StreamedIn)
end)
workspace.Castle.Touched:Connect(function(otherPart)
    local player = game.Players:GetPlayerFromCharacter(otherPart.Parent)
    if player then
        player:RequestStreamAroundAsync(Vector3.new(0, 0, 500))
    end
end)
";
    let script = crate::from_lua(source).expect("every line has a block");
    let lua = translate(&script).unwrap().source;
    assert!(lua.contains("StreamedIn:Connect"), "{lua}");
    assert!(lua.contains("RequestStreamAroundAsync"), "{lua}");
    assert!(
        lua.contains("Enum.StreamingState.StreamedIn"),
        "the check round trips: {lua}"
    );
    let mut stack = script.blocks.clone();
    let mut kinds = Vec::new();
    while let Some(block) = stack.pop() {
        for body in block.kind.bodies() {
            stack.extend(body.iter().cloned());
        }
        kinds.push(block.kind);
    }
    assert!(
        kinds
            .iter()
            .any(|kind| matches!(kind, BlockKind::WhenStreamedIn { .. })),
        "{kinds:?}"
    );
    assert!(
        kinds.iter().any(|kind| matches!(
            kind,
            BlockKind::RequestStreamAround {
                position: Expression::Vector3 { z, .. },
                ..
            } if *z == 500.0
        )),
        "{kinds:?}"
    );
}
