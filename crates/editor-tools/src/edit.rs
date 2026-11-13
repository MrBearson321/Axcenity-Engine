//! What the Properties panel and the panels built like it (the UI
//! Designer's inspector) hand the app: an [`Edit`] the user made, on a
//! [`Field`] of an Instance. The panels never change the world themselves.

use axcenity_core::Value;

/// Something about an Instance that can be changed: its name or one property.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Name,
    Property(&'static str),
}

/// A change made in the Properties panel.
#[derive(Clone, Debug, PartialEq)]
pub enum Edit {
    Name(String),
    Property(&'static str, Value),
    /// Asks for a texture file for this map property (such as a
    /// SurfaceAppearance's `ColorMap`) with the file picker. The app does
    /// that, and sets the property.
    PickTexture(&'static str),
    /// Asks for a lookup table or palette file for this ColorCorrectionEffect
    /// property (`LookupTable` or `Palette`), like [`Edit::PickTexture`].
    PickGradingFile(&'static str),
    /// Asks for an audio file for the Sound's `SoundId`, like [`Edit::PickTexture`].
    PickAudio,
    /// Asks for a state machine file for the Animator's `StateMachine`.
    PickController,
    /// Opens the Animator's state machine in the Animation tab.
    OpenController,
    /// Asks for a clip file for an Animation's `Clip`.
    PickClip,
    /// Opens the Animation's clip in the Animation tab.
    OpenClip,
    /// Asks for a behavior tree file for a NavigationAgent's `BehaviorTree`.
    PickBehaviorTree,
    /// Opens the agent's behavior tree in the Behavior Tree tab.
    OpenBehaviorTree,
    /// Asks for the code file of a script inside an object (its
    /// `SourceFile`, G2), with the ending its class needs.
    PickSourceFile,
    /// Opens the script's `SourceFile` in a tab.
    OpenSourceFile,
    /// Rename or move the file a script inside an object takes its code
    /// from, pointing every script that uses it at the new path.
    MoveSourceFile,
    /// Asks for a sky image file for Lighting's `SkyImage` with the file
    /// picker, then asks whether to use it.
    PickSkyImage,
    /// Asks whether to use the sky image at this project path (dragged onto
    /// the row from the Assets panel).
    AskSkyImage(String),
    /// Puts the engine's default sky into the project (if it isn't there)
    /// and asks whether to use it, with the sun moved to match.
    UseDefaultSky,
    /// Lets the sky follow the time of day: clears Lighting's `SkyImage`
    /// and adds an Atmosphere to Lighting if it has none, for sunrises and
    /// sunsets as `ClockTime` changes.
    UseTimeOfDaySky,
    /// Animates the selected Part, Model or UI element: an Animator, a new
    /// clip and an Animation playing it, opened in the Animation tab.
    Animate,
    /// Plays the selected Sound in the editor, to hear it without pressing
    /// Play, or (false) stops that.
    PreviewSound(bool),
    /// Makes the selected Ragdoll's bodies and joints from its MeshPart's
    /// skeleton, replacing what was in it.
    BuildRagdoll,
    /// Cuts the selected Fracture's part into its pieces and saves them.
    GenerateFracture,
    /// Turns what the selected SplineMesh or SplineScatter made into
    /// ordinary Instances (Phase 74).
    BakeSpline,
    /// Flattens the Terrain under the selected Spline (or the one the
    /// selection is in), road-wide (Phase 74).
    FlattenTerrainAlong,
    /// Opens the selected MeshPart's model in the Model Studio.
    OpenInModelStudio,
    /// Opens the selected MeshPart's `.glb` in Blender (R15.4).
    OpenInBlender,
    /// During Play, pins (true) or unpins a value edited during Play, so
    /// Stop copies it into the edit world (see `play::kept`).
    Keep(Field, bool),
    /// Sets an attribute, or removes it (`None`).
    Attribute(String, Option<Value>),
    AddTag(String),
    RemoveTag(String),
    /// Sets this overridden property of a placed prefab copy back to the
    /// prefab's value (Phase 49).
    RevertOverride(String),
    /// Writes this overridden property into the copy's prefab.
    ApplyOverride(String),
    /// Sets this property of each selected Instance back to its class's
    /// default (↺ with several selected).
    Reset(&'static str),
}
