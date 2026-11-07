//! The ribbon's dropdowns: one button that opens a short menu of what it
//! inserts, in place of a button for every class. The Explorer's Insert
//! menu has every class too.

use super::*;

/// A ribbon button named `label` that opens a menu of `sections` (a line
/// between each). The one picked is noted in `chosen`. Off during Play.
pub(super) fn insert_menu(
    ui: &mut egui::Ui,
    (icon, label): (&str, &str),
    hover: &str,
    sections: &[&[Insertable]],
    playing: bool,
    chosen: &mut Option<Insertable>,
) {
    let button = ui
        .add_enabled(!playing, RibbonButton::new(icon, label).dropdown())
        .on_hover_text(hover)
        .on_disabled_hover_text("Press Stop to edit.");
    egui::Popup::menu(&button).show(|ui| {
        insert_items(ui, sections, chosen);
    });
}

/// The menu items for `sections`, with a line between each.
pub(super) fn insert_items(
    ui: &mut egui::Ui,
    sections: &[&[Insertable]],
    chosen: &mut Option<Insertable>,
) {
    for (index, section) in sections.iter().enumerate() {
        if index > 0 {
            ui.separator();
        }
        for &what in *section {
            if ui
                .button(explorer::insert_item(ui, what))
                .on_hover_text(insert_hint(what))
                .clicked()
            {
                *chosen = Some(what);
            }
        }
    }
}

/// What inserting `what` from the ribbon does, for its tooltip.
pub(super) fn insert_hint(what: Insertable) -> String {
    let text = match what {
        Insertable::Part(_) => {
            return format!(
                "Insert a {} part at the camera's focus point, inside the selected Model or \
                 Folder if there is one.",
                what.label().to_lowercase()
            );
        }
        Insertable::Sound => "Insert a Sound into the selected part or the Workspace.",
        Insertable::SoundGroup => {
            "Insert a SoundGroup (an audio bus) into the selected SoundGroup or SoundService. \
             Set a Sound's SoundGroup to play it through one."
        }
        Insertable::ReverbZone => {
            "Insert a Reverb Zone: while the camera is inside it, spatial Sounds take on its \
             reverb."
        }
        Insertable::ParticleEmitter => {
            "Insert a ParticleEmitter into the selected part. With no part selected it stands \
             on its own at the camera's focus point, shown as an icon you can drag."
        }
        Insertable::LensEffect(ClassName::DepthOfFieldEffect) => {
            "Insert a DepthOfFieldEffect into Lighting: what's nearer or farther than its \
             FocusDistance blurs, as through a camera's lens."
        }
        Insertable::LensEffect(ClassName::VignetteEffect) => {
            "Insert a VignetteEffect into Lighting: the picture darkens (or takes its Color) \
             toward the corners."
        }
        Insertable::LensEffect(ClassName::FilmGrainEffect) => {
            "Insert a FilmGrainEffect into Lighting: moving noise over the picture, as on film."
        }
        Insertable::LensEffect(ClassName::ChromaticAberrationEffect) => {
            "Insert a ChromaticAberrationEffect into Lighting: red and blue drift apart toward \
             the picture's edges."
        }
        Insertable::LensEffect(ClassName::MotionBlurEffect) => {
            "Insert a MotionBlurEffect into Lighting: what moves across the view streaks along \
             its motion, as through a camera's shutter."
        }
        Insertable::LensEffect(_) => {
            "Insert a SharpenEffect into Lighting: crisper edges between colors, useful with a \
             render scale below 100%."
        }
        Insertable::GraphicsVolume => {
            "Insert a Graphics Volume: an invisible box that changes the look while the camera \
             is inside it, fading in over its BlendDistance. Put Exposure, Fog, Bloom, Color \
             Correction or an Atmosphere inside it; where volumes overlap, the higher Priority \
             wins."
        }
        Insertable::ExposureEffect => {
            "Insert an ExposureEffect into the selected Graphics Volume (or a new one): the \
             exposure inside it, in place of Lighting's."
        }
        Insertable::FogEffect => {
            "Insert a FogEffect into the selected Graphics Volume (or a new one): distance fog \
             inside it, in place of the world's."
        }
        Insertable::BloomEffect => {
            "Insert a BloomEffect into the selected Graphics Volume (or a new one): how bright \
             light blooms inside it, in place of Lighting's bloom."
        }
        Insertable::PointLight | Insertable::SpotLight => {
            "Insert a light into the selected part. With no part selected it stands on its own \
             at the camera's focus point, as in Unreal: no lamp, just an icon you can drag."
        }
        Insertable::RectLight => {
            "Insert a RectLight: light from a rectangle, out of its front only, like a window \
             or a softbox. In a part it sits on the side it faces; with no part selected it \
             stands on its own at the camera's focus point."
        }
        Insertable::ReflectionProbe => {
            "Insert a ReflectionProbe into the selected part: shiny things in the part's box \
             reflect what's around them there, once baked. With no part selected, an invisible \
             room-sized box is made for it."
        }
        Insertable::LightProbeVolume => {
            "Insert a LightProbeVolume into the selected part: a grid of probes in the part's \
             box lights everything there, still or moving, with the light bounced around it, \
             once baked. With no part selected, an invisible room-sized box is made for it."
        }
        Insertable::Clouds => {
            "Insert Clouds into Lighting: a layer of cloud high in the sky, lit by the sun, \
             whose shadows fall on the world. Workspace's GlobalWind carries them along."
        }
        Insertable::OutlineEffect => {
            "Insert an OutlineEffect into Lighting: lines along the edges of everything solid, \
             around silhouettes and along creases, with no setup per part. A material file's \
             outline draws one around its parts alone."
        }
        Insertable::ColorCorrectionEffect => {
            "Insert a ColorCorrectionEffect into Lighting (or the selected Graphics Volume): \
             changes the finished picture's colors (brightness, contrast, saturation and a \
             tint), and can look them up in a .cube table, posterize them or snap them to a \
             palette."
        }
        Insertable::Weather => {
            "Insert Weather into Lighting: rain or snow falling around the camera, wetting \
             surfaces or covering their tops in snow. Changes ease in over its TransitionTime, \
             and Workspace's GlobalWind blows the rain and snow."
        }
        Insertable::Atmosphere => {
            "Insert an Atmosphere into Lighting: the sky is drawn as sunlight scattered by air, \
             reddening toward sunset and darkening at night as Lighting's ClockTime goes round, \
             and it colors the sun's light."
        }
        Insertable::ThirdPersonController | Insertable::FirstPersonController => {
            return format!(
                "Insert a {} on the selected model or part. With nothing selected, or on a \
                 character that already has a controller, makes a simple character.",
                what.class()
            );
        }
        Insertable::Health => {
            "Insert a Health into the selected character's Model: how much it can take. \
             Scripts call TakeDamage and Heal, and it fires Died once at 0. Turn on FallDamage \
             to hurt it when it lands hard."
        }
        Insertable::DamageZone => {
            "Insert a Damage Zone: a see-through part that takes DamagePerSecond from every \
             Health whose Model is inside it during Play."
        }
        Insertable::Seat => {
            "Insert a Seat: a part a character sits on when it walks into it during Play. Jump \
             stands it up."
        }
        Insertable::ProximityPrompt => {
            "Insert a Prompt into the selected part or Attachment. During Play, walking up to \
             it shows ActionText with the Interact key, and holding that for HoldDuration fires \
             Triggered."
        }
        Insertable::ClickDetector => {
            "Insert a Click Detector into the selected part. During Play, clicking the part \
             fires MouseClick, and the cursor changes over it."
        }
        Insertable::Character => {
            "Insert a Character: six welded parts with a Humanoid, a third-person controller, \
             Health, Inventory and an Animator with Idle, Walk, Run, Jump, Fall and Sit states."
        }
        Insertable::Humanoid => {
            "Insert a Humanoid into the selected character's Model: Roblox-style properties \
             and methods over its controller, Health, Inventory and Animator."
        }
        Insertable::SpawnLocation => {
            "Insert a SpawnLocation: a pad players' characters spawn on during Play, and \
             respawn on after dying. Turn Neutral off and set its TeamColor to keep it for one \
             team."
        }
        Insertable::Team => {
            "Insert a Team into Teams: players join the AutoAssignable team with the fewest \
             players, and spawn on SpawnLocations of its TeamColor."
        }
        Insertable::Tool => {
            "Insert a Tool: a Handle part with a Grip. During Play, a character with an \
             Inventory picks it up by touching it, number keys equip it in the right hand, and \
             clicking fires Activated."
        }
        Insertable::Inventory => {
            "Insert an Inventory into the selected character's Model: the Tools it carries, \
             shown in the hotbar during Play."
        }
        Insertable::Vehicle => {
            "Insert a ready-to-drive car. During Play, walk into its seat on the left to drive \
             with the move keys or a stick; Jump gets out."
        }
        Insertable::VehicleSeat => {
            "Insert a Vehicle Seat: weld it to a car's chassis to drive it, on wheels inserted \
             into the chassis."
        }
        Insertable::VehicleWheel => {
            "Insert a Wheel into the selected part: a spring and tyre that a Vehicle Seat \
             welded to that part drives on."
        }
        Insertable::NavigationAgent => {
            "Insert a NavigationAgent on the selected model or part, which walks it along the \
             navigation mesh to its Destination during Play. With nothing selected, makes a \
             simple character for it."
        }
        Insertable::Spline => {
            "Insert a Spline in front of you: a curve through its points, for a Spline Follower \
             to carry a part along, an agent to patrol or a cutscene camera to ride. Click it to \
             drag its points; Alt-drag a point to add one after it."
        }
        Insertable::SplineFollower => {
            "Insert a Spline Follower into the selected part or model: during Play it carries \
             it along its Spline at Speed, there and back by default. Characters standing on \
             it ride along."
        }
        Insertable::SplineMesh => {
            "Insert a Spline Mesh into the selected Spline (or a new one): it bends a model \
             along the curve, repeated to fill it, for roads, pipes, fences and rails. Click a \
             model in the Assets panel while it's selected to use it."
        }
        Insertable::SplineScatter => {
            "Insert a Spline Scatter into the selected Spline (or a new one): it places models \
             along the curve, or inside a closed one, from a seed. Click models in the Assets \
             panel while it's selected to add them."
        }
        Insertable::Checkpoint => {
            "Insert a Checkpoint on top of the selected part: a place \
             SaveService:RestoreCheckpoint (or the \"go back to checkpoint\" block) puts the \
             player character back at."
        }
        Insertable::Camera => {
            "Insert a Camera where you're looking from: a named viewpoint a LocalScript cuts to \
             with workspace.CurrentCamera.CFrame = viewpoint.CFrame. It isn't drawn in the game."
        }
        Insertable::Attachment => {
            "Insert an Attachment in the selected part, at its middle: a point and direction on \
             the part that moves with it."
        }
        Insertable::ScreenGui => "Insert a ScreenGui: a layer of UI over the game.",
        Insertable::PageLayout => {
            "Insert a PageLayout. Its child Frames are pages; a script or the show-page block \
             sets CurrentPage."
        }
        Insertable::Page => {
            "Insert a page Frame into the selected PageLayout (a PageLayout and a ScreenGui are \
             made if there isn't one)."
        }
        Insertable::BillboardGui => {
            "Insert a BillboardGui into the selected part. It floats in the world and is drawn \
             with the screen UI."
        }
        Insertable::HealthBar => {
            "Insert health bars: one floating over the nearest character with a Health, and \
             one at the top left of the screen. Both follow that Health with no script, styled \
             by health-bar.css."
        }
        _ if Insertable::UI.contains(&what) => {
            return format!(
                "Insert a {} into the selected ScreenGui or Frame (or the first ScreenGui; one is \
                 made if there's none). Drag its handles in the viewport, and style it with CSS \
                 in the Style section of Properties.",
                what.label()
            );
        }
        _ => return format!("Insert a {}.", what.label()),
    };
    text.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_insertable_on_the_ribbon_says_what_it_does() {
        let ribbon = [
            &Insertable::PARTS[..],
            &Insertable::UI,
            &Insertable::AUDIO,
            &Insertable::LIGHTS,
            &Insertable::PROBES,
            &Insertable::SKY,
            &Insertable::LENS,
            &Insertable::VOLUMES,
            &Insertable::EFFECTS,
            &Insertable::CHARACTERS,
            &Insertable::AGENTS,
            &Insertable::SPLINES,
            &Insertable::CHECKPOINTS,
            &Insertable::HEALTH,
            &Insertable::SEATS,
            &Insertable::INTERACTION,
            &Insertable::HUMANOIDS,
            &Insertable::SPAWNING,
            &Insertable::TOOLS,
            &Insertable::VEHICLES,
            &Insertable::CAMERAS,
            &Insertable::CONSTRAINTS,
        ];
        for what in ribbon.into_iter().flatten() {
            let hint = insert_hint(*what);
            assert!(hint.len() > 25, "{what:?}: {hint}");
        }
    }
}
