use super::*;

/// What each property means. A test checks every property in `core` is here.
pub(super) const PROPERTY_DOCS: &[(ClassName, &str, &str)] = &[
    (
        ClassName::Frame,
        "Selectable",
        "If true, keyboard and controller navigation can focus it (CSS :focus). On by default for buttons, text boxes, sliders, toggles and dropdowns.",
    ),
    (
        ClassName::Frame,
        "NextSelectionUp",
        "The Id of the element focus moves to when up is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::Frame,
        "NextSelectionDown",
        "The Id of the element focus moves to when down is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::Frame,
        "NextSelectionLeft",
        "The Id of the element focus moves to when left is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::Frame,
        "NextSelectionRight",
        "The Id of the element focus moves to when right is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::TextLabel,
        "Selectable",
        "If true, keyboard and controller navigation can focus it (CSS :focus). On by default for buttons, text boxes, sliders, toggles and dropdowns.",
    ),
    (
        ClassName::TextLabel,
        "NextSelectionUp",
        "The Id of the element focus moves to when up is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::TextLabel,
        "NextSelectionDown",
        "The Id of the element focus moves to when down is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::TextLabel,
        "NextSelectionLeft",
        "The Id of the element focus moves to when left is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::TextLabel,
        "NextSelectionRight",
        "The Id of the element focus moves to when right is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::TextButton,
        "Selectable",
        "If true, keyboard and controller navigation can focus it (CSS :focus). On by default for buttons, text boxes, sliders, toggles and dropdowns.",
    ),
    (
        ClassName::TextButton,
        "NextSelectionUp",
        "The Id of the element focus moves to when up is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::TextButton,
        "NextSelectionDown",
        "The Id of the element focus moves to when down is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::TextButton,
        "NextSelectionLeft",
        "The Id of the element focus moves to when left is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::TextButton,
        "NextSelectionRight",
        "The Id of the element focus moves to when right is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::ImageLabel,
        "Selectable",
        "If true, keyboard and controller navigation can focus it (CSS :focus). On by default for buttons, text boxes, sliders, toggles and dropdowns.",
    ),
    (
        ClassName::ImageLabel,
        "NextSelectionUp",
        "The Id of the element focus moves to when up is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::ImageLabel,
        "NextSelectionDown",
        "The Id of the element focus moves to when down is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::ImageLabel,
        "NextSelectionLeft",
        "The Id of the element focus moves to when left is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::ImageLabel,
        "NextSelectionRight",
        "The Id of the element focus moves to when right is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::TextBox,
        "Selectable",
        "If true, keyboard and controller navigation can focus it (CSS :focus). On by default for buttons, text boxes, sliders, toggles and dropdowns.",
    ),
    (
        ClassName::TextBox,
        "NextSelectionUp",
        "The Id of the element focus moves to when up is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::TextBox,
        "NextSelectionDown",
        "The Id of the element focus moves to when down is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::TextBox,
        "NextSelectionLeft",
        "The Id of the element focus moves to when left is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::TextBox,
        "NextSelectionRight",
        "The Id of the element focus moves to when right is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::PageLayout,
        "Selectable",
        "If true, keyboard and controller navigation can focus it (CSS :focus). On by default for buttons, text boxes, sliders, toggles and dropdowns.",
    ),
    (
        ClassName::PageLayout,
        "NextSelectionUp",
        "The Id of the element focus moves to when up is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::PageLayout,
        "NextSelectionDown",
        "The Id of the element focus moves to when down is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::PageLayout,
        "NextSelectionLeft",
        "The Id of the element focus moves to when left is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::PageLayout,
        "NextSelectionRight",
        "The Id of the element focus moves to when right is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::Table,
        "Selectable",
        "If true, keyboard and controller navigation can focus it (CSS :focus). On by default for buttons, text boxes, sliders, toggles and dropdowns.",
    ),
    (
        ClassName::Table,
        "NextSelectionUp",
        "The Id of the element focus moves to when up is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::Table,
        "NextSelectionDown",
        "The Id of the element focus moves to when down is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::Table,
        "NextSelectionLeft",
        "The Id of the element focus moves to when left is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::Table,
        "NextSelectionRight",
        "The Id of the element focus moves to when right is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::ScrollingFrame,
        "Selectable",
        "If true, keyboard and controller navigation can focus it (CSS :focus). On by default for buttons, text boxes, sliders, toggles and dropdowns.",
    ),
    (
        ClassName::ScrollingFrame,
        "NextSelectionUp",
        "The Id of the element focus moves to when up is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::ScrollingFrame,
        "NextSelectionDown",
        "The Id of the element focus moves to when down is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::ScrollingFrame,
        "NextSelectionLeft",
        "The Id of the element focus moves to when left is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::ScrollingFrame,
        "NextSelectionRight",
        "The Id of the element focus moves to when right is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::Slider,
        "Selectable",
        "If true, keyboard and controller navigation can focus it (CSS :focus). On by default for buttons, text boxes, sliders, toggles and dropdowns.",
    ),
    (
        ClassName::Slider,
        "NextSelectionUp",
        "The Id of the element focus moves to when up is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::Slider,
        "NextSelectionDown",
        "The Id of the element focus moves to when down is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::Slider,
        "NextSelectionLeft",
        "The Id of the element focus moves to when left is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::Slider,
        "NextSelectionRight",
        "The Id of the element focus moves to when right is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::Toggle,
        "Selectable",
        "If true, keyboard and controller navigation can focus it (CSS :focus). On by default for buttons, text boxes, sliders, toggles and dropdowns.",
    ),
    (
        ClassName::Toggle,
        "NextSelectionUp",
        "The Id of the element focus moves to when up is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::Toggle,
        "NextSelectionDown",
        "The Id of the element focus moves to when down is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::Toggle,
        "NextSelectionLeft",
        "The Id of the element focus moves to when left is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::Toggle,
        "NextSelectionRight",
        "The Id of the element focus moves to when right is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::Dropdown,
        "Selectable",
        "If true, keyboard and controller navigation can focus it (CSS :focus). On by default for buttons, text boxes, sliders, toggles and dropdowns.",
    ),
    (
        ClassName::Dropdown,
        "NextSelectionUp",
        "The Id of the element focus moves to when up is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::Dropdown,
        "NextSelectionDown",
        "The Id of the element focus moves to when down is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::Dropdown,
        "NextSelectionLeft",
        "The Id of the element focus moves to when left is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::Dropdown,
        "NextSelectionRight",
        "The Id of the element focus moves to when right is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::ProgressBar,
        "Selectable",
        "If true, keyboard and controller navigation can focus it (CSS :focus). On by default for buttons, text boxes, sliders, toggles and dropdowns.",
    ),
    (
        ClassName::ProgressBar,
        "NextSelectionUp",
        "The Id of the element focus moves to when up is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::ProgressBar,
        "NextSelectionDown",
        "The Id of the element focus moves to when down is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::ProgressBar,
        "NextSelectionLeft",
        "The Id of the element focus moves to when left is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::ProgressBar,
        "NextSelectionRight",
        "The Id of the element focus moves to when right is pressed while this is focused. Empty picks the nearest selectable element that way.",
    ),
    (
        ClassName::ScreenGui,
        "Class",
        "Space-separated CSS class names used to style this UI Instance.",
    ),
    (
        ClassName::ScreenGui,
        "Id",
        "The CSS identifier used to style this UI Instance.",
    ),
    (
        ClassName::ScreenGui,
        "Enabled",
        "If true, the UI in this ScreenGui is drawn.",
    ),
    (
        ClassName::ScreenGui,
        "ScaleMode",
        "Fixed keeps positions and sizes as authored. Fit lays this ScreenGui out at DesignWidth by DesignHeight, then scales it uniformly so it stays proportional and inside the screen. Scale scales it the same way but lays it out over the whole screen, so elements anchored to an edge stay there.",
    ),
    (
        ClassName::ScreenGui,
        "DesignWidth",
        "The width, in pixels, this ScreenGui is designed at: Fit and Scale scale it from this size. Ignored when ScaleMode is Fixed.",
    ),
    (
        ClassName::ScreenGui,
        "DesignHeight",
        "The height, in pixels, this ScreenGui is designed at: Fit and Scale scale it from this size. Ignored when ScaleMode is Fixed.",
    ),
    (
        ClassName::Frame,
        "Class",
        "Space-separated CSS class names used to style this UI Instance.",
    ),
    (
        ClassName::Frame,
        "Id",
        "The CSS identifier used to style this UI Instance.",
    ),
    (
        ClassName::Frame,
        "Position",
        "The top-left position relative to the parent, in scale and pixel offsets.",
    ),
    (
        ClassName::Frame,
        "Size",
        "The size relative to the parent, in scale and pixel offsets.",
    ),
    (
        ClassName::Frame,
        "BackgroundColor3",
        "The frame's background color.",
    ),
    (
        ClassName::Frame,
        "BackgroundTransparency",
        "The background opacity: 0 is opaque and 1 is transparent.",
    ),
    (
        ClassName::Frame,
        "Visible",
        "If false, this frame and its children are hidden.",
    ),
    (
        ClassName::TextLabel,
        "Class",
        "Space-separated CSS class names used to style this UI Instance.",
    ),
    (
        ClassName::TextLabel,
        "Id",
        "The CSS identifier used to style this UI Instance.",
    ),
    (
        ClassName::TextLabel,
        "Position",
        "The top-left position relative to the parent, in scale and pixel offsets.",
    ),
    (
        ClassName::TextLabel,
        "Size",
        "The size relative to the parent, in scale and pixel offsets.",
    ),
    (
        ClassName::TextLabel,
        "BackgroundColor3",
        "The label's background color.",
    ),
    (
        ClassName::TextLabel,
        "BackgroundTransparency",
        "The background opacity: 0 is opaque and 1 is transparent.",
    ),
    (ClassName::TextLabel, "Text", "The text to display."),
    (ClassName::TextLabel, "TextColor3", "The text color."),
    (ClassName::TextLabel, "TextSize", "The text size in pixels."),
    (
        ClassName::TextLabel,
        "TextScaled",
        "If true, the text is drawn at the largest size up to TextSize (or CSS font-size) that fits its box.",
    ),
    (
        ClassName::TextLabel,
        "RichText",
        "If true, tags in Text style parts of it: <b>, <i>, <u>, <s>, <font color size face weight>, <stroke color thickness>, <uc> and <br/>. Write &lt; and &amp; for < and &.",
    ),
    (
        ClassName::TextLabel,
        "Visible",
        "If false, this label is hidden.",
    ),
    (
        ClassName::TextButton,
        "Class",
        "Space-separated CSS class names used to style this UI Instance.",
    ),
    (
        ClassName::TextButton,
        "Id",
        "The CSS identifier used to style this UI Instance.",
    ),
    (
        ClassName::TextButton,
        "Position",
        "The top-left position relative to the parent, in scale and pixel offsets.",
    ),
    (
        ClassName::TextButton,
        "Size",
        "The size relative to the parent, in scale and pixel offsets.",
    ),
    (
        ClassName::TextButton,
        "BackgroundColor3",
        "The button's background color.",
    ),
    (
        ClassName::TextButton,
        "BackgroundTransparency",
        "The background opacity: 0 is opaque and 1 is transparent.",
    ),
    (ClassName::TextButton, "Text", "The text to display."),
    (ClassName::TextButton, "TextColor3", "The text color."),
    (
        ClassName::TextButton,
        "TextSize",
        "The text size in pixels.",
    ),
    (
        ClassName::TextButton,
        "TextScaled",
        "If true, the text is drawn at the largest size up to TextSize (or CSS font-size) that fits its box.",
    ),
    (
        ClassName::TextButton,
        "RichText",
        "If true, tags in Text style parts of it: <b>, <i>, <u>, <s>, <font color size face weight>, <stroke color thickness>, <uc> and <br/>. Write &lt; and &amp; for < and &.",
    ),
    (
        ClassName::TextButton,
        "Image",
        "The project-relative path of an icon. Empty shows no picture. With Text, the icon sits beside the label.",
    ),
    (
        ClassName::TextButton,
        "ImageTransparency",
        "The icon opacity: 0 is opaque and 1 is transparent.",
    ),
    (
        ClassName::TextButton,
        "Visible",
        "If false, this button is hidden and cannot be activated.",
    ),
    (
        ClassName::TextBox,
        "Class",
        "Space-separated CSS class names used to style this UI Instance.",
    ),
    (
        ClassName::TextBox,
        "Id",
        "The CSS identifier used to style this UI Instance.",
    ),
    (
        ClassName::TextBox,
        "Position",
        "The top-left position relative to the parent, in scale and pixel offsets.",
    ),
    (
        ClassName::TextBox,
        "Size",
        "The size relative to the parent, in scale and pixel offsets.",
    ),
    (
        ClassName::TextBox,
        "BackgroundColor3",
        "The box's background color.",
    ),
    (
        ClassName::TextBox,
        "BackgroundTransparency",
        "The background opacity: 0 is opaque and 1 is transparent.",
    ),
    (ClassName::TextBox, "Text", "The text the player has typed."),
    (
        ClassName::TextBox,
        "Placeholder",
        "Grey text shown while Text is empty.",
    ),
    (ClassName::TextBox, "TextColor3", "The text color."),
    (ClassName::TextBox, "TextSize", "The text size in pixels."),
    (
        ClassName::TextBox,
        "TextScaled",
        "If true, the text is drawn at the largest size up to TextSize (or CSS font-size) that fits its box.",
    ),
    (
        ClassName::TextBox,
        "RichText",
        "If true, tags in Text style parts of it: <b>, <i>, <u>, <s>, <font color size face weight>, <stroke color thickness>, <uc> and <br/>. Write &lt; and &amp; for < and &.",
    ),
    (
        ClassName::TextBox,
        "MultiLine",
        "If false, Enter finishes typing. If true, Enter starts a new line and clicking away finishes typing.",
    ),
    (
        ClassName::TextBox,
        "Visible",
        "If false, this text box is hidden.",
    ),
    (
        ClassName::PageLayout,
        "Class",
        "Space-separated CSS class names used to style this UI Instance.",
    ),
    (
        ClassName::PageLayout,
        "Id",
        "The CSS identifier used to style this UI Instance.",
    ),
    (
        ClassName::PageLayout,
        "Position",
        "The top-left position relative to the parent, in scale and pixel offsets.",
    ),
    (
        ClassName::PageLayout,
        "Size",
        "The size relative to the parent, in scale and pixel offsets.",
    ),
    (
        ClassName::PageLayout,
        "BackgroundColor3",
        "The layout's background color.",
    ),
    (
        ClassName::PageLayout,
        "BackgroundTransparency",
        "The background opacity: 0 is opaque and 1 is transparent.",
    ),
    (
        ClassName::PageLayout,
        "Visible",
        "If false, this layout and its pages are hidden.",
    ),
    (
        ClassName::PageLayout,
        "CurrentPage",
        "The Name of the child Frame to show. Empty shows the first child Frame. A name that matches no Frame is rejected and the current page stays.",
    ),
    (
        ClassName::Table,
        "Class",
        "Space-separated CSS class names used to style this UI Instance.",
    ),
    (
        ClassName::Table,
        "Id",
        "The CSS identifier used to style this UI Instance.",
    ),
    (
        ClassName::Table,
        "Position",
        "The top-left position relative to the parent, in scale and pixel offsets.",
    ),
    (
        ClassName::Table,
        "Size",
        "The size relative to the parent, in scale and pixel offsets.",
    ),
    (
        ClassName::Table,
        "BackgroundColor3",
        "The table's background color.",
    ),
    (
        ClassName::Table,
        "BackgroundTransparency",
        "The background opacity: 0 is opaque and 1 is transparent.",
    ),
    (
        ClassName::Table,
        "Visible",
        "If false, this table is hidden.",
    ),
    (
        ClassName::Table,
        "Columns",
        "How many cells sit in one row. Cells are the table's children, left to right then top to bottom, each the same size.",
    ),
    (
        ClassName::Frame,
        "Interactable",
        "If false, it takes no hover or clicks (a button doesn't fire Activated, a text box can't be typed in) and CSS :disabled styles it.",
    ),
    (
        ClassName::Frame,
        "Tooltip",
        "Text shown in a small box near the pointer after a short hover. Empty shows none. Style the box with a CSS Tooltip rule, such as #play Tooltip.",
    ),
    (
        ClassName::TextLabel,
        "Interactable",
        "If false, it takes no hover or clicks (a button doesn't fire Activated, a text box can't be typed in) and CSS :disabled styles it.",
    ),
    (
        ClassName::TextLabel,
        "Tooltip",
        "Text shown in a small box near the pointer after a short hover. Empty shows none. Style the box with a CSS Tooltip rule, such as #play Tooltip.",
    ),
    (
        ClassName::TextButton,
        "Interactable",
        "If false, it takes no hover or clicks (a button doesn't fire Activated, a text box can't be typed in) and CSS :disabled styles it.",
    ),
    (
        ClassName::TextButton,
        "Tooltip",
        "Text shown in a small box near the pointer after a short hover. Empty shows none. Style the box with a CSS Tooltip rule, such as #play Tooltip.",
    ),
    (
        ClassName::ImageLabel,
        "Interactable",
        "If false, it takes no hover or clicks (a button doesn't fire Activated, a text box can't be typed in) and CSS :disabled styles it.",
    ),
    (
        ClassName::ImageLabel,
        "Tooltip",
        "Text shown in a small box near the pointer after a short hover. Empty shows none. Style the box with a CSS Tooltip rule, such as #play Tooltip.",
    ),
    (
        ClassName::TextBox,
        "Interactable",
        "If false, it takes no hover or clicks (a button doesn't fire Activated, a text box can't be typed in) and CSS :disabled styles it.",
    ),
    (
        ClassName::TextBox,
        "Tooltip",
        "Text shown in a small box near the pointer after a short hover. Empty shows none. Style the box with a CSS Tooltip rule, such as #play Tooltip.",
    ),
    (
        ClassName::PageLayout,
        "Interactable",
        "If false, it takes no hover or clicks (a button doesn't fire Activated, a text box can't be typed in) and CSS :disabled styles it.",
    ),
    (
        ClassName::PageLayout,
        "Tooltip",
        "Text shown in a small box near the pointer after a short hover. Empty shows none. Style the box with a CSS Tooltip rule, such as #play Tooltip.",
    ),
    (
        ClassName::Table,
        "Interactable",
        "If false, it takes no hover or clicks (a button doesn't fire Activated, a text box can't be typed in) and CSS :disabled styles it.",
    ),
    (
        ClassName::Table,
        "Tooltip",
        "Text shown in a small box near the pointer after a short hover. Empty shows none. Style the box with a CSS Tooltip rule, such as #play Tooltip.",
    ),
    (
        ClassName::TextButton,
        "HoverSound",
        "A sound file (a path in the project) played once when the pointer comes onto the button. Empty plays nothing.",
    ),
    (
        ClassName::TextButton,
        "ClickSound",
        "A sound file (a path in the project) played once when the button is clicked. Empty plays nothing.",
    ),
    (
        ClassName::BillboardGui,
        "Class",
        "Space-separated CSS class names used to style descendants of this billboard.",
    ),
    (
        ClassName::BillboardGui,
        "Id",
        "The CSS identifier used to style this billboard.",
    ),
    (
        ClassName::BillboardGui,
        "Enabled",
        "If false, this billboard is not drawn.",
    ),
    (
        ClassName::BillboardGui,
        "Size",
        "The on-screen size, in scale of the viewport plus a pixel offset.",
    ),
    (
        ClassName::BillboardGui,
        "StudsOffset",
        "World studs added to the parent part's position. The offset is not rotated with the part.",
    ),
    (
        ClassName::BillboardGui,
        "MaxDistance",
        "The billboard hides past this many studs from the camera. 0 shows it at any distance.",
    ),
    (
        ClassName::ImageLabel,
        "Class",
        "Space-separated CSS class names used to style this UI Instance.",
    ),
    (
        ClassName::ImageLabel,
        "Id",
        "The CSS identifier used to style this UI Instance.",
    ),
    (
        ClassName::ImageLabel,
        "Position",
        "The top-left position relative to the parent, in scale and pixel offsets.",
    ),
    (
        ClassName::ImageLabel,
        "Size",
        "The size relative to the parent, in scale and pixel offsets.",
    ),
    (
        ClassName::ImageLabel,
        "BackgroundColor3",
        "The image label's background color.",
    ),
    (
        ClassName::ImageLabel,
        "BackgroundTransparency",
        "The background opacity: 0 is opaque and 1 is transparent.",
    ),
    (
        ClassName::ImageLabel,
        "Image",
        "The project-relative path of the image to display.",
    ),
    (
        ClassName::ImageLabel,
        "ImageTransparency",
        "The image opacity: 0 is opaque and 1 is transparent.",
    ),
    (
        ClassName::ImageLabel,
        "Visible",
        "If false, this image is hidden.",
    ),
    (
        ClassName::Sound,
        "SoundId",
        "The audio file inside the project folder, such as \"assets/audio/chime.wav\".",
    ),
    (
        ClassName::Sound,
        "Volume",
        "Playback volume as a non-negative amplitude; 1 is the original level and 0 is silent.",
    ),
    (
        ClassName::Sound,
        "Looped",
        "If true, playback repeats until Stop is called or the game session stops.",
    ),
    (
        ClassName::Sound,
        "PlaybackSpeed",
        "How fast the sound plays: 1 is normal, 2 twice as fast (and an octave higher), 0.5 half. Changes apply while it plays.",
    ),
    (
        ClassName::Sound,
        "Playing",
        "True while the sound plays. Set it to false to pause and true to carry on. Saved as true, the sound plays as soon as the game starts, with no script needed.",
    ),
    (
        ClassName::Sound,
        "Spatial",
        "If true, the sound is placed in the world and panned as the camera moves. If false, it plays the same in both ears.",
    ),
    (
        ClassName::Sound,
        "Position",
        "Where a spatial sound plays from, in studs, when it is not inside a Part, MeshPart or Terrain. Inside one of those, that ancestor's Position is used instead.",
    ),
    (
        ClassName::Sound,
        "RollOffMinDistance",
        "A spatial sound stays at full volume within this many studs of the camera. 10 by default. Not used while Spatial is false.",
    ),
    (
        ClassName::Sound,
        "RollOffMaxDistance",
        "A spatial sound is silent from this many studs out. 150 by default. Kept at least as large as RollOffMinDistance.",
    ),
    (
        ClassName::Workspace,
        "GlobalWind",
        "The wind everywhere, in studs a second. It carries particles from emitters with WindAffectsDrag and pushes unanchored parts with WindAffected.",
    ),
    (
        ClassName::Lighting,
        "ShadowsEnabled",
        "If false, the sun casts no shadows. Turn this off on low-end machines.",
    ),
    (
        ClassName::Lighting,
        "ShadowDistance",
        "How far from the camera sun shadows reach, in studs. Shorter distances give sharper shadows.",
    ),
    (
        ClassName::Lighting,
        "ShadowSoftness",
        "How soft shadow edges are, from 0 (crisp) to 1 (very soft). Edges stay sharp where a part touches what it shades and soften further away, like real shadows.",
    ),
    (
        ClassName::Lighting,
        "AmbientOcclusionIntensity",
        "How strongly ambient occlusion darkens the ambient and sky light in corners and creases between parts, from 0 (not at all) to 2. 1 is normal. It's drawn only where the player's graphics settings turn ambient occlusion on, and never darkens sunlight, lamps or glowing parts.",
    ),
    (
        ClassName::Lighting,
        "SkyTopColor",
        "The sky's color straight up.",
    ),
    (
        ClassName::Lighting,
        "SkyHorizonColor",
        "The sky's color at the horizon.",
    ),
    (
        ClassName::Lighting,
        "GroundColor",
        "The color of the ground below the horizon, as the sky and reflections see it.",
    ),
    (
        ClassName::Lighting,
        "EnvironmentIntensity",
        "How strongly the sky lights and is reflected by parts. 0 turns it off.",
    ),
    (
        ClassName::Lighting,
        "PostProcessingEnabled",
        "If false, the scene is drawn with no tone mapping or bloom.",
    ),
    (
        ClassName::Lighting,
        "Exposure",
        "Brightens or darkens the whole image before tone mapping. 1 is neutral.",
    ),
    (
        ClassName::Lighting,
        "ToneMapping",
        "The curve that turns bright light into colors a screen can show. Classic (worlds made before it existed) keeps colors under 0.6 as they are and flattens brighter ones; Neutral (new worlds) keeps colors close to how they were picked and rolls highlights off without changing their hue; Filmic adds contrast for a cinematic look.",
    ),
    (
        ClassName::Lighting,
        "BloomEnabled",
        "If false, bright parts don't glow.",
    ),
    (
        ClassName::Lighting,
        "BloomIntensity",
        "How strong the glow around bright parts is.",
    ),
    (
        ClassName::Lighting,
        "BloomThreshold",
        "How bright a pixel must be before it glows.",
    ),
    (
        ClassName::PointLight,
        "Enabled",
        "If false, the light is off.",
    ),
    (ClassName::PointLight, "Color", "The light's color."),
    (
        ClassName::PointLight,
        "Brightness",
        "How bright the light is.",
    ),
    (
        ClassName::PointLight,
        "Range",
        "How far the light reaches, in studs.",
    ),
    (
        ClassName::SpotLight,
        "Enabled",
        "If false, the light is off.",
    ),
    (ClassName::SpotLight, "Color", "The light's color."),
    (
        ClassName::SpotLight,
        "Brightness",
        "How bright the light is.",
    ),
    (
        ClassName::SpotLight,
        "Range",
        "How far the light reaches, in studs.",
    ),
    (
        ClassName::SpotLight,
        "Angle",
        "The full width of the cone, in degrees.",
    ),
    (
        ClassName::SpotLight,
        "Direction",
        "The way the beam points, relative to the parent part.",
    ),
    (
        ClassName::SpotLight,
        "Shadows",
        "If true, parts in the beam cast shadows. Only the few spot lights nearest the camera do, as the graphics settings allow.",
    ),
    (
        ClassName::ParticleEmitter,
        "Enabled",
        "If false, it stops giving off particles; those already out live on.",
    ),
    (
        ClassName::Animator,
        "StateMachine",
        "The state machine file it plays on its parent, inside the project folder, such as \"assets/animations/hero/hero.statemachine.json\". Empty plays only the AnimationTracks loaded on it. The Animator tab makes and edits them.",
    ),
    (
        ClassName::Animator,
        "Enabled",
        "If false, everything it animates holds still: the animator stops moving on.",
    ),
    (
        ClassName::Animator,
        "Speed",
        "Plays everything this many times as fast (1 is normal, 0 pauses).",
    ),
    (
        ClassName::Animation,
        "Clip",
        "The clip file inside the project folder, such as \"assets/animations/Door/Open.anim.json\".",
    ),
    (
        ClassName::Animation,
        "AutoPlay",
        "If true, the Animator on the same parent plays it by itself when the game starts.",
    ),
    (
        ClassName::Animation,
        "Enabled",
        "If false, playback pauses at its current time; set true to continue.",
    ),
    (
        ClassName::Animation,
        "Speed",
        "Plays the clip this many times as fast (1 is normal, 0 pauses).",
    ),
    (
        ClassName::Animation,
        "Looped",
        "If true, playback repeats instead of firing Completed at the end.",
    ),
    (
        ClassName::ParticleEmitter,
        "ParticleType",
        "The built-in picture each particle shows: Circle, Spark, Smoke, Fire, Star or Magic. Texture, if set, is used instead.",
    ),
    (
        ClassName::ParticleEmitter,
        "Texture",
        "An image file inside the project folder each particle shows instead of ParticleType, such as \"assets/particles/spark_01.png\" (Kenney's free Particle Pack works well). Its alpha is the shape; it's tinted by Color.",
    ),
    (
        ClassName::ParticleEmitter,
        "Rate",
        "How many particles a second it gives off while Enabled.",
    ),
    (
        ClassName::ParticleEmitter,
        "Lifetime",
        "How many seconds each particle lives (each one a little more or less).",
    ),
    (
        ClassName::ParticleEmitter,
        "Speed",
        "How fast particles leave, in studs a second, along Direction.",
    ),
    (
        ClassName::ParticleEmitter,
        "Direction",
        "The way particles leave, relative to the parent part (up by default).",
    ),
    (
        ClassName::ParticleEmitter,
        "SpreadAngle",
        "How wide the cone particles leave in is, in degrees: 0 is a straight line, 360 every way.",
    ),
    (
        ClassName::ParticleEmitter,
        "Acceleration",
        "Added to every particle's speed each second, in world studs (gravity is Vector3.new(0, -196.2, 0)).",
    ),
    (
        ClassName::ParticleEmitter,
        "Drag",
        "How quickly particles slow down: 0 never, higher sooner.",
    ),
    (
        ClassName::ParticleEmitter,
        "WindAffectsDrag",
        "If true, Drag slows particles toward workspace.GlobalWind instead of to a stop, so the wind carries them. Needs a Drag above 0.",
    ),
    (
        ClassName::ParticleEmitter,
        "Size",
        "How many studs across a particle is when it's born.",
    ),
    (
        ClassName::ParticleEmitter,
        "SizeEnd",
        "How many studs across a particle is when it dies; it grows or shrinks smoothly.",
    ),
    (
        ClassName::ParticleEmitter,
        "Color",
        "A particle's color when it's born. It tints the picture.",
    ),
    (
        ClassName::ParticleEmitter,
        "ColorEnd",
        "A particle's color when it dies; it fades smoothly from Color.",
    ),
    (
        ClassName::ParticleEmitter,
        "Transparency",
        "From 0 (solid) to 1 (invisible) when a particle is born.",
    ),
    (
        ClassName::ParticleEmitter,
        "TransparencyEnd",
        "From 0 (solid) to 1 (invisible) when a particle dies.",
    ),
    (
        ClassName::ParticleEmitter,
        "Rotation",
        "How far each particle is turned when it's born, in degrees.",
    ),
    (
        ClassName::ParticleEmitter,
        "RotSpeed",
        "How fast particles spin, in degrees a second.",
    ),
    (
        ClassName::ParticleEmitter,
        "LightEmission",
        "From 0 (particles paint over what's behind them, like smoke) to 1 (they add their light, like fire and sparks).",
    ),
    (
        ClassName::ParticleEmitter,
        "Brightness",
        "How brightly particles glow: 1 is their color as it is; higher values bloom.",
    ),
    (
        ClassName::Part,
        "Anchored",
        "If true, the part stays where it is. If false, it falls and collides with other parts.",
    ),
    (
        ClassName::Part,
        "Shape",
        "Block, Ball, Cylinder (along X) or Wedge.",
    ),
    (
        ClassName::Part,
        "Position",
        "Where the part's center is, in studs. Setting it on an unanchored part teleports it and stops it moving.",
    ),
    (
        ClassName::Part,
        "Orientation",
        "Rotation in degrees around X, Y and Z, applied Z first, then X, then Y.",
    ),
    (
        ClassName::Part,
        "Size",
        "The part's size in studs along X, Y and Z.",
    ),
    (ClassName::Part, "Color", "The part's color."),
    (
        ClassName::Part,
        "Material",
        "Plastic, Metal, Glass (see-through) or Neon (glows). Setting it resets Metalness and Roughness.",
    ),
    (
        ClassName::Part,
        "MaterialVariant",
        "A material file in the project, such as \"assets/materials/brick.material.json\", drawn in place of Material, Metalness, Roughness and Emission; Color tints it and Transparency fades it. A SurfaceAppearance inside the part wins over it. Empty for none.",
    ),
    (
        ClassName::Part,
        "Metalness",
        "From 0 (not metal) to 1 (metal).",
    ),
    (
        ClassName::Part,
        "Roughness",
        "From 0 (mirror-smooth) to 1 (matte).",
    ),
    (
        ClassName::Part,
        "CastShadow",
        "If false, the part casts no sun shadow. It's still shaded by other parts' shadows.",
    ),
    (
        ClassName::Part,
        "Transparency",
        "From 0 (as solid as its material) to 1 (invisible).",
    ),
    (
        ClassName::Part,
        "CanCollide",
        "If false, other parts pass through it. It still fires Touched. An unanchored part that can't collide falls through the floor.",
    ),
    (
        ClassName::Part,
        "WindAffected",
        "If true and the part isn't anchored, workspace.GlobalWind pushes it, harder for parts with bigger faces.",
    ),
    (
        ClassName::Part,
        "EmissiveColor",
        "The color the part glows in when Emission is above 0.",
    ),
    (
        ClassName::Part,
        "Emission",
        "How brightly the part glows in EmissiveColor: 0 is no glow, 1 as bright as a white surface in full light, higher values bloom.",
    ),
    (
        ClassName::Part,
        "TileSize",
        "Studs one copy of a SurfaceAppearance's textures covers. 0 leaves the shape's own coordinates unscaled.",
    ),
    (
        ClassName::MeshPart,
        "CastShadow",
        "If false, the part casts no sun shadow. It's still shaded by other parts' shadows.",
    ),
    (
        ClassName::MeshPart,
        "Transparency",
        "From 0 (as solid as its model) to 1 (invisible).",
    ),
    (
        ClassName::MeshPart,
        "CanCollide",
        "If false, other parts pass through it. It still fires Touched.",
    ),
    (
        ClassName::MeshPart,
        "WindAffected",
        "If true and the part isn't anchored, workspace.GlobalWind pushes it, harder for parts with bigger faces.",
    ),
    (
        ClassName::MeshPart,
        "Anchored",
        "If true, the part stays where it is. If false, it falls and collides with other parts.",
    ),
    (
        ClassName::MeshPart,
        "MeshId",
        "The model's glTF or FBX file, inside the project folder, such as \"assets/models/chair.glb\"; `#` after it picks objects in the file (\"assets/models/props.fbx#Bench\"). The editor's Import Model sets it.",
    ),
    (
        ClassName::MeshPart,
        "MaterialVariant",
        "A material file in the project, such as \"assets/materials/brick.material.json\", drawn in place of every material of the model, except one a SurfaceAppearance replaces. Empty for none.",
    ),
    (
        ClassName::MeshPart,
        "Position",
        "Where the center of the model's box is, in studs. Setting it on an unanchored part teleports it and stops it moving.",
    ),
    (
        ClassName::MeshPart,
        "Orientation",
        "Rotation in degrees around X, Y and Z, applied Z first, then X, then Y.",
    ),
    (
        ClassName::MeshPart,
        "Size",
        "The size of the model's box in studs along X, Y and Z. The model stretches to fill it.",
    ),
    (
        ClassName::SurfaceAppearance,
        "MaterialName",
        "On a MeshPart, the model's material whose maps this replaces. Unused on a Part. Import Model sets it.",
    ),
    (
        ClassName::SurfaceAppearance,
        "Color",
        "Multiplies the material's color (and its ColorMap).",
    ),
    (
        ClassName::SurfaceAppearance,
        "ColorMap",
        "The base color texture, a file inside the project folder such as \"assets/textures/wood.png\". Empty for none.",
    ),
    (
        ClassName::SurfaceAppearance,
        "NormalMap",
        "The normal map's file, for bumps and grooves. Empty for none.",
    ),
    (
        ClassName::SurfaceAppearance,
        "MetalnessMap",
        "The metalness map's file (read from MetalnessChannel). Empty for the model's own metalness.",
    ),
    (
        ClassName::SurfaceAppearance,
        "RoughnessMap",
        "The roughness map's file (read from RoughnessChannel). Empty for the model's own roughness.",
    ),
    (
        ClassName::SurfaceAppearance,
        "EmissiveMap",
        "The file of colors the surface glows in. Empty for the model's own glow.",
    ),
    (
        ClassName::SurfaceAppearance,
        "AmbientOcclusionMap",
        "The ambient occlusion map's file: dark where ambient light can't reach. Empty for none.",
    ),
    (
        ClassName::SurfaceAppearance,
        "OpacityMap",
        "The opacity map's file (read from OpacityChannel): the surface is cut out where it's below half. Empty for the color map's alpha.",
    ),
    (
        ClassName::SurfaceAppearance,
        "SpecularMap",
        "How strongly non-metal parts shine, for older materials: mid grey is the usual. Empty for none.",
    ),
    (
        ClassName::SurfaceAppearance,
        "NormalStrength",
        "How far the normal map tips the surface: 1 as made, 0 flat.",
    ),
    (
        ClassName::SurfaceAppearance,
        "NormalMapDirectX",
        "True for normal maps made for DirectX, whose green points the other way.",
    ),
    (
        ClassName::SurfaceAppearance,
        "MetalnessChannel",
        "Which channel of MetalnessMap holds metalness, for packed textures.",
    ),
    (
        ClassName::SurfaceAppearance,
        "RoughnessChannel",
        "Which channel of RoughnessMap holds roughness, for packed textures.",
    ),
    (
        ClassName::SurfaceAppearance,
        "InvertRoughness",
        "True if RoughnessMap is a glossiness or smoothness map: white is smooth.",
    ),
    (
        ClassName::SurfaceAppearance,
        "AmbientOcclusionChannel",
        "Which channel of AmbientOcclusionMap holds it, for packed textures.",
    ),
    (
        ClassName::SurfaceAppearance,
        "AmbientOcclusionStrength",
        "How much AmbientOcclusionMap darkens, from 0 to 1.",
    ),
    (
        ClassName::SurfaceAppearance,
        "OpacityChannel",
        "Which channel of OpacityMap holds opacity.",
    ),
    (
        ClassName::SurfaceAppearance,
        "InvertOpacity",
        "True if OpacityMap is white where the surface is see-through.",
    ),
    (
        ClassName::SurfaceAppearance,
        "EmissiveStrength",
        "Multiplies the glow.",
    ),
    (
        ClassName::SurfaceAppearance,
        "Tiling",
        "How many times as often the maps repeat, across and down: `Vector2.new(2, 2)` repeats them twice as often. On a Part, on top of its TileSize.",
    ),
    (
        ClassName::SurfaceAppearance,
        "Offset",
        "Shifts the maps across and down, in copies of the texture: `Vector2.new(0.5, 0)` moves them half a copy across.",
    ),
    (
        ClassName::Terrain,
        "Position",
        "The middle of the terrain's bottom: its lowest possible ground.",
    ),
    (
        ClassName::Terrain,
        "Size",
        "Width (X), height range (Y) and length (Z) in studs. Heights are stored as parts of the height range, so changing Y stretches the hills.",
    ),
    (
        ClassName::Terrain,
        "CastShadow",
        "Whether the terrain casts shadows.",
    ),
    (
        ClassName::Terrain,
        "CanCollide",
        "Whether parts and characters stand on the terrain.",
    ),
    (
        ClassName::Terrain,
        "DataFolder",
        "The project folder its heights and painted materials are saved in, such as \"assets/terrain/Terrain\". The editor picks it.",
    ),
    (
        ClassName::TerrainLayer,
        "Color",
        "Tints the layer's color map, or is its color without one.",
    ),
    (
        ClassName::TerrainLayer,
        "ColorMap",
        "The layer's color texture, relative to the project folder. Empty means none.",
    ),
    (
        ClassName::TerrainLayer,
        "NormalMap",
        "The layer's normal map. Empty means none.",
    ),
    (
        ClassName::TerrainLayer,
        "RoughnessMap",
        "The layer's roughness map. Empty means none.",
    ),
    (
        ClassName::TerrainLayer,
        "MetalnessMap",
        "The layer's metalness map. Empty means none.",
    ),
    (
        ClassName::TerrainLayer,
        "AmbientOcclusionMap",
        "The layer's ambient occlusion map. Empty means none.",
    ),
    (
        ClassName::TerrainLayer,
        "HeightMap",
        "The layer's height map: light is high. Where layers meet, the higher one shows through.",
    ),
    (
        ClassName::TerrainLayer,
        "Metalness",
        "0 to 1: how metallic the layer is, without a metalness map.",
    ),
    (
        ClassName::TerrainLayer,
        "Roughness",
        "0 to 1: how rough the layer is, without a roughness map.",
    ),
    (
        ClassName::TerrainLayer,
        "NormalStrength",
        "Scales the normal map's bumps.",
    ),
    (
        ClassName::TerrainLayer,
        "NormalMapDirectX",
        "True for a DirectX-style normal map, whose green points down.",
    ),
    (
        ClassName::TerrainLayer,
        "RoughnessChannel",
        "Which channel of RoughnessMap holds the roughness.",
    ),
    (
        ClassName::TerrainLayer,
        "InvertRoughness",
        "Reads RoughnessMap as smoothness (1 minus the value).",
    ),
    (
        ClassName::TerrainLayer,
        "MetalnessChannel",
        "Which channel of MetalnessMap holds the metalness.",
    ),
    (
        ClassName::TerrainLayer,
        "AmbientOcclusionChannel",
        "Which channel of AmbientOcclusionMap holds the occlusion.",
    ),
    (
        ClassName::TerrainLayer,
        "TileSize",
        "Studs one copy of the textures covers.",
    ),
    (
        ClassName::TerrainLayer,
        "TextureRotation",
        "Turns the textures, in degrees.",
    ),
    (
        ClassName::TerrainLayer,
        "Triplanar",
        "Projects the textures from the sides too, so cliffs don't stretch them.",
    ),
    (
        ClassName::TerrainLayer,
        "AntiTiling",
        "0 to 1: mixes in a larger, turned copy of the textures so repeats show less.",
    ),
    (
        ClassName::TerrainLayer,
        "HeightBlend",
        "0 to 1: how sharply layers meet where they have height maps.",
    ),
    (
        ClassName::TerrainLayer,
        "AutoEnabled",
        "Whether the layer lays itself down by its height and slope rule.",
    ),
    (
        ClassName::TerrainLayer,
        "MinHeight",
        "The rule's lowest height, in studs above the terrain's bottom.",
    ),
    (
        ClassName::TerrainLayer,
        "MaxHeight",
        "The rule's highest height, in studs above the terrain's bottom.",
    ),
    (
        ClassName::TerrainLayer,
        "HeightFade",
        "Studs over which the rule fades at its heights.",
    ),
    (
        ClassName::TerrainLayer,
        "MinSlope",
        "The rule's gentlest slope, in degrees from flat.",
    ),
    (
        ClassName::TerrainLayer,
        "MaxSlope",
        "The rule's steepest slope, in degrees from flat.",
    ),
    (
        ClassName::TerrainLayer,
        "SlopeFade",
        "Degrees over which the rule fades at its slopes.",
    ),
    (
        ClassName::TerrainLayer,
        "RuleStrength",
        "0 to 1: how fully the layer covers where its rule holds.",
    ),
    (
        ClassName::TerrainLayer,
        "RuleNoise",
        "0 to 1: breaks the rule's edges up with noise.",
    ),
    (
        ClassName::TerrainLayer,
        "NoiseScale",
        "Studs across the rule's noise patches.",
    ),
    (
        ClassName::ThirdPersonController,
        "Movement",
        "\"Walk\", \"Run\" or \"Crouch\", or any other name for a custom mode: the controller stops moving the character, and scripts and blocks move it. Set \"\" to clear a custom mode; the controller puts back the built-in mode it had. Set and read from a script or the set-property and property blocks.",
    ),
    (
        ClassName::ThirdPersonController,
        "WalkSpeed",
        "How fast Movement \"Walk\" moves, in studs a second, on flat ground.",
    ),
    (
        ClassName::ThirdPersonController,
        "RunSpeed",
        "How fast Movement \"Run\" moves, in studs a second, on flat ground.",
    ),
    (
        ClassName::ThirdPersonController,
        "JumpHeight",
        "How high a jump rises, in studs. Space jumps during Play.",
    ),
    (
        ClassName::ThirdPersonController,
        "Jump",
        "Set true to jump once if the character is on the ground. The controller sets it back to false on its next step, so a jump in the air does nothing.",
    ),
    (
        ClassName::ThirdPersonController,
        "MaxSlopeAngle",
        "The steepest slope the character walks up, in degrees from flat. A steeper slope stops it.",
    ),
    (
        ClassName::ThirdPersonController,
        "MaxStepHeight",
        "The tallest ledge the character steps onto without jumping, in studs. A taller one stops it.",
    ),
    (
        ClassName::ThirdPersonController,
        "Crouch",
        "Set true to crouch and false to stand. Holding C crouches too. The character stays crouched while something overhead is in the way of standing.",
    ),
    (
        ClassName::ThirdPersonController,
        "CrouchHeight",
        "How tall the character is while crouched, in studs. Its parts squash toward its feet, and the camera drops as far as its top.",
    ),
    (
        ClassName::ThirdPersonController,
        "CrouchSpeedScale",
        "0 to 1: how much of WalkSpeed or RunSpeed the character keeps while crouched.",
    ),
    (
        ClassName::ThirdPersonController,
        "CameraDistance",
        "How far behind the character the camera sits during Play, in studs. The mouse wheel zooms it between CameraMinZoomDistance and CameraMaxZoomDistance and keeps the zoom here.",
    ),
    (
        ClassName::ThirdPersonController,
        "CameraMinZoomDistance",
        "The closest the mouse wheel zooms the camera, in studs (at least 0.5). Set it to CameraMaxZoomDistance to lock the distance.",
    ),
    (
        ClassName::ThirdPersonController,
        "CameraMaxZoomDistance",
        "The farthest the mouse wheel zooms the camera out, in studs.",
    ),
    (
        ClassName::ThirdPersonController,
        "CameraHeight",
        "How far above the character's feet the camera looks, in studs.",
    ),
    (
        ClassName::FirstPersonController,
        "Movement",
        "\"Walk\", \"Run\" or \"Crouch\", or any other name for a custom mode: the controller stops moving the character, and scripts and blocks move it. Set \"\" to clear a custom mode; the controller puts back the built-in mode it had. Set and read from a script or the set-property and property blocks.",
    ),
    (
        ClassName::FirstPersonController,
        "WalkSpeed",
        "How fast Movement \"Walk\" moves, in studs a second, on flat ground.",
    ),
    (
        ClassName::FirstPersonController,
        "RunSpeed",
        "How fast Movement \"Run\" moves, in studs a second, on flat ground.",
    ),
    (
        ClassName::FirstPersonController,
        "JumpHeight",
        "How high a jump rises, in studs. Space jumps during Play.",
    ),
    (
        ClassName::FirstPersonController,
        "Jump",
        "Set true to jump once if the character is on the ground. The controller sets it back to false on its next step, so a jump in the air does nothing.",
    ),
    (
        ClassName::FirstPersonController,
        "MaxSlopeAngle",
        "The steepest slope the character walks up, in degrees from flat. A steeper slope stops it.",
    ),
    (
        ClassName::FirstPersonController,
        "MaxStepHeight",
        "The tallest ledge the character steps onto without jumping, in studs. A taller one stops it.",
    ),
    (
        ClassName::FirstPersonController,
        "Crouch",
        "Set true to crouch and false to stand. Holding C crouches too. The character stays crouched while something overhead is in the way of standing.",
    ),
    (
        ClassName::FirstPersonController,
        "CrouchHeight",
        "How tall the character is while crouched, in studs. Its parts squash toward its feet, and the eyes drop as far as its top.",
    ),
    (
        ClassName::FirstPersonController,
        "CrouchSpeedScale",
        "0 to 1: how much of WalkSpeed or RunSpeed the character keeps while crouched.",
    ),
    (
        ClassName::FirstPersonController,
        "EyeHeight",
        "How far above the character's feet its eyes are, in studs. The camera sits there during Play.",
    ),
    (
        ClassName::NavigationAgent,
        "Destination",
        "Where the agent walks to, along the navigation mesh, during Play. Setting a new one finds a new path. A point off the mesh, or with no walkable route to it, leaves the agent where it is: Status becomes \"Unreachable\" and Output says why. The editor starts it where the agent stands. Set it from a script or the \"walk to\" block.",
    ),
    (
        ClassName::NavigationAgent,
        "WalkSpeed",
        "How fast the agent walks its path, in studs a second. It steers round anything that moves into its path, and rejoins the path past it.",
    ),
    (
        ClassName::NavigationAgent,
        "Velocity",
        "Studs a second. Not zero, the agent moves straight that way along the ground, with no path, and stops at the edge of walkable ground. Destination follows it, so setting Velocity back to zero leaves it where it is. Y is ignored. Set it from a script or the \"set velocity\" block.",
    ),
    (
        ClassName::NavigationAgent,
        "Status",
        "\"Moving\" while it walks a path or moves at Velocity, \"Reached\" at its Destination, \"Unreachable\" when no path leads to its Destination, \"Blocked\" when something in the way of its path has no way round (it stops, Output says so, and it carries on once the way clears), and \"Idle\" after Velocity stopped it. Set by the game during Play; read it from a script or the property block.",
    ),
    (
        ClassName::NavigationAgent,
        "SightRange",
        "How far the agent sees the player character, in studs, from its eyes near the top of its body. Parts that collide block its sight. 0 sees nothing.",
    ),
    (
        ClassName::NavigationAgent,
        "SightAngle",
        "How wide the agent sees, in degrees across, centred on the way it faces: the way it last moved, or its body's front before it has moved. 360 sees all round; 0 sees nothing.",
    ),
    (
        ClassName::NavigationAgent,
        "HearingRange",
        "How far the agent hears the player character, in studs, through walls. It hears a Sound playing anywhere inside the player character's model. 0 hears nothing.",
    ),
    (
        ClassName::NavigationAgent,
        "SeesPlayer",
        "True while the agent sees the player character (the one whose controller owns the camera). Set by the game during Play; PlayerSensed fires when it turns true.",
    ),
    (
        ClassName::NavigationAgent,
        "HearsPlayer",
        "True while the agent hears a Sound playing in the player character. Set by the game during Play; PlayerSensed fires when it turns true.",
    ),
    (
        ClassName::NavigationAgent,
        "BehaviorTree",
        "The behavior tree the agent runs during Play: a .behaviortree.json file in the project, made in the Behavior Tree tab. Every frame, before the agent walks, the tree decides where it goes (it can test SeesPlayer and HearsPlayer, set Destination, chase the player character and wait), so it may overwrite a Destination a script set. Empty runs none, and scripts set Destination themselves. A file that isn't a behavior tree, or has problems, runs nothing and Output says why once.",
    ),
    (
        ClassName::Checkpoint,
        "Position",
        "Where SaveService:RestoreCheckpoint stands the player character: the middle of its feet goes here.",
    ),
    (
        ClassName::Frame,
        "ZIndex",
        "Its place among its siblings: higher is drawn over lower and takes the pointer first, and equal ones keep their order in the tree. CSS `z-index` wins over it. Whole numbers; 1 by default.",
    ),
    (
        ClassName::TextLabel,
        "ZIndex",
        "Its place among its siblings: higher is drawn over lower and takes the pointer first, and equal ones keep their order in the tree. CSS `z-index` wins over it. Whole numbers; 1 by default.",
    ),
    (
        ClassName::TextButton,
        "ZIndex",
        "Its place among its siblings: higher is drawn over lower and takes the pointer first, and equal ones keep their order in the tree. CSS `z-index` wins over it. Whole numbers; 1 by default.",
    ),
    (
        ClassName::ImageLabel,
        "ZIndex",
        "Its place among its siblings: higher is drawn over lower and takes the pointer first, and equal ones keep their order in the tree. CSS `z-index` wins over it. Whole numbers; 1 by default.",
    ),
    (
        ClassName::TextBox,
        "ZIndex",
        "Its place among its siblings: higher is drawn over lower and takes the pointer first, and equal ones keep their order in the tree. CSS `z-index` wins over it. Whole numbers; 1 by default.",
    ),
    (
        ClassName::PageLayout,
        "ZIndex",
        "Its place among its siblings: higher is drawn over lower and takes the pointer first, and equal ones keep their order in the tree. CSS `z-index` wins over it. Whole numbers; 1 by default.",
    ),
    (
        ClassName::Table,
        "ZIndex",
        "Its place among its siblings: higher is drawn over lower and takes the pointer first, and equal ones keep their order in the tree. CSS `z-index` wins over it. Whole numbers; 1 by default.",
    ),
    (
        ClassName::ScreenGui,
        "DisplayOrder",
        "Where this ScreenGui is drawn among the others: higher is drawn over lower and takes the pointer first, and equal ones keep their order in the tree.",
    ),
    (
        ClassName::ScrollingFrame,
        "Class",
        "Space-separated CSS class names used to style this UI Instance.",
    ),
    (
        ClassName::ScrollingFrame,
        "Id",
        "The CSS identifier used to style this UI Instance.",
    ),
    (
        ClassName::ScrollingFrame,
        "Position",
        "The top-left position relative to the parent, in scale and pixel offsets.",
    ),
    (
        ClassName::ScrollingFrame,
        "Size",
        "The size of the window onto the canvas, relative to the parent, in scale and pixel offsets.",
    ),
    (
        ClassName::ScrollingFrame,
        "BackgroundColor3",
        "The frame's background color.",
    ),
    (
        ClassName::ScrollingFrame,
        "BackgroundTransparency",
        "The background opacity: 0 is opaque and 1 is transparent.",
    ),
    (
        ClassName::ScrollingFrame,
        "Visible",
        "If false, this frame and its children are hidden.",
    ),
    (
        ClassName::ScrollingFrame,
        "Interactable",
        "If false, it takes no hover or clicks and CSS :disabled styles it.",
    ),
    (
        ClassName::ScrollingFrame,
        "Tooltip",
        "Text shown in a small box near the pointer after a short hover. Empty shows none. Style the box with a CSS Tooltip rule, such as #play Tooltip.",
    ),
    (
        ClassName::ScrollingFrame,
        "ZIndex",
        "Its place among its siblings: higher is drawn over lower and takes the pointer first, and equal ones keep their order in the tree. CSS `z-index` wins over it. Whole numbers; 1 by default.",
    ),
    (
        ClassName::ScrollingFrame,
        "CanvasSize",
        "The area the children are laid out on: scale of the frame's own size plus pixels. It's never smaller than the frame. The default, 2 scale down, is twice as tall as the frame.",
    ),
    (
        ClassName::ScrollingFrame,
        "CanvasPosition",
        "How far the canvas is scrolled, in pixels from its top left: `Vector2.new(0, 120)` is 120 pixels down. Kept between 0 and as far as the canvas goes. The wheel, the scroll bars and dragging the content set it.",
    ),
    (
        ClassName::ScrollingFrame,
        "ScrollingDirection",
        "Which ways it scrolls: \"X\" (across), \"Y\" (up and down) or \"XY\" (both).",
    ),
    (
        ClassName::ScrollingFrame,
        "ScrollBarThickness",
        "How thick the scroll bars are, in pixels; 0 hides them. Style them with CSS: ScrollingFrame::scrollbar for the track and ::thumb for the part you drag.",
    ),
    (
        ClassName::ScrollingFrame,
        "AutomaticCanvasSize",
        "\"None\", \"X\", \"Y\" or \"XY\": the canvas grows that way to fit the children, past CanvasSize.",
    ),
    (
        ClassName::Frame,
        "Rotation",
        "Degrees clockwise: it and everything in it are drawn turned around its middle, and clicks hit the turned shape. Layout doesn't change, so its Position and Size are those of the upright box. 0 by default.",
    ),
    (
        ClassName::TextLabel,
        "Rotation",
        "Degrees clockwise: it and everything in it are drawn turned around its middle, and clicks hit the turned shape. Layout doesn't change, so its Position and Size are those of the upright box. 0 by default.",
    ),
    (
        ClassName::TextButton,
        "Rotation",
        "Degrees clockwise: it and everything in it are drawn turned around its middle, and clicks hit the turned shape. Layout doesn't change, so its Position and Size are those of the upright box. 0 by default.",
    ),
    (
        ClassName::ImageLabel,
        "Rotation",
        "Degrees clockwise: it and everything in it are drawn turned around its middle, and clicks hit the turned shape. Layout doesn't change, so its Position and Size are those of the upright box. 0 by default.",
    ),
    (
        ClassName::TextBox,
        "Rotation",
        "Degrees clockwise: it and everything in it are drawn turned around its middle, and clicks hit the turned shape. Layout doesn't change, so its Position and Size are those of the upright box. 0 by default.",
    ),
    (
        ClassName::PageLayout,
        "Rotation",
        "Degrees clockwise: it and everything in it are drawn turned around its middle, and clicks hit the turned shape. Layout doesn't change, so its Position and Size are those of the upright box. 0 by default.",
    ),
    (
        ClassName::Table,
        "Rotation",
        "Degrees clockwise: it and everything in it are drawn turned around its middle, and clicks hit the turned shape. Layout doesn't change, so its Position and Size are those of the upright box. 0 by default.",
    ),
    (
        ClassName::ScrollingFrame,
        "Rotation",
        "Degrees clockwise: it and everything in it are drawn turned around its middle, and clicks hit the turned shape. Layout doesn't change, so its Position and Size are those of the upright box. 0 by default.",
    ),
    (
        ClassName::TextLabel,
        "TextTransparency",
        "How see-through its text is, from 0 (solid) to 1 (invisible); its outline and shadows fade with it. The background and picture keep their own transparency, and CSS opacity still fades the whole element.",
    ),
    (
        ClassName::TextButton,
        "TextTransparency",
        "How see-through its text is, from 0 (solid) to 1 (invisible); its outline and shadows fade with it. The background and picture keep their own transparency, and CSS opacity still fades the whole element.",
    ),
    (
        ClassName::TextBox,
        "TextTransparency",
        "How see-through its text is, from 0 (solid) to 1 (invisible); its outline and shadows fade with it. The background and picture keep their own transparency, and CSS opacity still fades the whole element.",
    ),
    (
        ClassName::ScreenGui,
        "ScreenInsets",
        "\"None\" lays it out over the whole screen. \"DeviceSafeInsets\" keeps it inside the device's safe area, clear of a phone's notch and home bar; a desktop screen has none, so it's the whole screen there.",
    ),
    (
        ClassName::Slider,
        "Class",
        "Space-separated CSS class names used to style this UI Instance.",
    ),
    (
        ClassName::Slider,
        "Id",
        "The CSS identifier used to style this UI Instance.",
    ),
    (
        ClassName::Slider,
        "Position",
        "The top-left position relative to the parent, in scale and pixel offsets.",
    ),
    (
        ClassName::Slider,
        "Size",
        "The size relative to the parent, in scale and pixel offsets.",
    ),
    (
        ClassName::Slider,
        "BackgroundColor3",
        "The background color behind its parts.",
    ),
    (
        ClassName::Slider,
        "BackgroundTransparency",
        "The background opacity: 0 is opaque and 1 is transparent.",
    ),
    (ClassName::Slider, "Visible", "If false, it's hidden."),
    (
        ClassName::Slider,
        "Interactable",
        "If false, the pointer can't change it, it takes no hover, and CSS :disabled styles it. Scripts can still set its Value.",
    ),
    (
        ClassName::Slider,
        "Tooltip",
        "Text shown in a small box near the pointer after a short hover. Empty shows none. Style the box with a CSS Tooltip rule, such as #play Tooltip.",
    ),
    (
        ClassName::Slider,
        "ZIndex",
        "Its place among its siblings: higher is drawn over lower and takes the pointer first, and equal ones keep their order in the tree. CSS `z-index` wins over it. Whole numbers; 1 by default.",
    ),
    (
        ClassName::Slider,
        "Rotation",
        "Degrees clockwise: it's drawn turned around its middle, and clicks hit the turned shape. Layout doesn't change, so its Position and Size are those of the upright box. 0 by default.",
    ),
    (
        ClassName::Slider,
        "Value",
        "The number picked, from MinValue to MaxValue. Dragging the thumb or clicking the track sets it; setting it from a script moves the thumb. Each change fires ValueChanged.",
    ),
    (
        ClassName::Slider,
        "MinValue",
        "The number at the track's start (its left, or its bottom when Vertical).",
    ),
    (
        ClassName::Slider,
        "MaxValue",
        "The number at the track's end (its right, or its top when Vertical).",
    ),
    (
        ClassName::Slider,
        "Step",
        "Dragging picks numbers this far apart, counted from MinValue. 0 picks any number.",
    ),
    (
        ClassName::Slider,
        "Orientation",
        "\"Horizontal\" (MinValue at the left) or \"Vertical\" (MinValue at the bottom). Style its parts with CSS: Slider::track, ::fill (the track up to the thumb) and ::thumb.",
    ),
    (
        ClassName::Toggle,
        "Class",
        "Space-separated CSS class names used to style this UI Instance.",
    ),
    (
        ClassName::Toggle,
        "Id",
        "The CSS identifier used to style this UI Instance.",
    ),
    (
        ClassName::Toggle,
        "Position",
        "The top-left position relative to the parent, in scale and pixel offsets.",
    ),
    (
        ClassName::Toggle,
        "Size",
        "The size relative to the parent, in scale and pixel offsets.",
    ),
    (
        ClassName::Toggle,
        "BackgroundColor3",
        "The background color behind its parts.",
    ),
    (
        ClassName::Toggle,
        "BackgroundTransparency",
        "The background opacity: 0 is opaque and 1 is transparent.",
    ),
    (ClassName::Toggle, "Visible", "If false, it's hidden."),
    (
        ClassName::Toggle,
        "Interactable",
        "If false, the pointer can't change it, it takes no hover, and CSS :disabled styles it. Scripts can still set its Value.",
    ),
    (
        ClassName::Toggle,
        "Tooltip",
        "Text shown in a small box near the pointer after a short hover. Empty shows none. Style the box with a CSS Tooltip rule, such as #play Tooltip.",
    ),
    (
        ClassName::Toggle,
        "ZIndex",
        "Its place among its siblings: higher is drawn over lower and takes the pointer first, and equal ones keep their order in the tree. CSS `z-index` wins over it. Whole numbers; 1 by default.",
    ),
    (
        ClassName::Toggle,
        "Rotation",
        "Degrees clockwise: it's drawn turned around its middle, and clicks hit the turned shape. Layout doesn't change, so its Position and Size are those of the upright box. 0 by default.",
    ),
    (
        ClassName::Toggle,
        "Value",
        "On (true) or off (false). A click flips it; setting it from a script flips it too. Each change fires ValueChanged. CSS :checked styles it while it's on.",
    ),
    (
        ClassName::Toggle,
        "Style",
        "\"Switch\" (a knob that slides across a track) or \"Checkbox\" (a box ticked while on). Style its parts with CSS: Toggle::track, ::thumb (a switch's knob) and ::check (a checkbox's tick).",
    ),
    (
        ClassName::Dropdown,
        "Class",
        "Space-separated CSS class names used to style this UI Instance.",
    ),
    (
        ClassName::Dropdown,
        "Id",
        "The CSS identifier used to style this UI Instance.",
    ),
    (
        ClassName::Dropdown,
        "Position",
        "The top-left position relative to the parent, in scale and pixel offsets.",
    ),
    (
        ClassName::Dropdown,
        "Size",
        "The size relative to the parent, in scale and pixel offsets.",
    ),
    (
        ClassName::Dropdown,
        "BackgroundColor3",
        "The background color behind its parts.",
    ),
    (
        ClassName::Dropdown,
        "BackgroundTransparency",
        "The background opacity: 0 is opaque and 1 is transparent.",
    ),
    (ClassName::Dropdown, "Visible", "If false, it's hidden."),
    (
        ClassName::Dropdown,
        "Interactable",
        "If false, the pointer can't change it, it takes no hover, and CSS :disabled styles it. Scripts can still set its Value.",
    ),
    (
        ClassName::Dropdown,
        "Tooltip",
        "Text shown in a small box near the pointer after a short hover. Empty shows none. Style the box with a CSS Tooltip rule, such as #play Tooltip.",
    ),
    (
        ClassName::Dropdown,
        "ZIndex",
        "Its place among its siblings: higher is drawn over lower and takes the pointer first, and equal ones keep their order in the tree. CSS `z-index` wins over it. Whole numbers; 1 by default.",
    ),
    (
        ClassName::Dropdown,
        "Rotation",
        "Degrees clockwise: it's drawn turned around its middle, and clicks hit the turned shape. Layout doesn't change, so its Position and Size are those of the upright box. 0 by default.",
    ),
    (
        ClassName::Dropdown,
        "Options",
        "The choices, one per line, in the order the list shows them.",
    ),
    (
        ClassName::Dropdown,
        "Value",
        "The chosen option's text; empty shows the Placeholder. Picking from the list sets it, and so can a script (to any text). Each change fires ValueChanged.",
    ),
    (
        ClassName::Dropdown,
        "Placeholder",
        "Shown faintly while nothing is chosen.",
    ),
    (
        ClassName::Dropdown,
        "TextColor3",
        "The color of the chosen option's text, unless CSS `color` sets it.",
    ),
    (
        ClassName::Dropdown,
        "TextSize",
        "The size of its text in pixels, unless CSS `font-size` sets it.",
    ),
    (
        ClassName::ProgressBar,
        "Class",
        "Space-separated CSS class names used to style this UI Instance.",
    ),
    (
        ClassName::ProgressBar,
        "Id",
        "The CSS identifier used to style this UI Instance.",
    ),
    (
        ClassName::ProgressBar,
        "Position",
        "The top-left position relative to the parent, in scale and pixel offsets.",
    ),
    (
        ClassName::ProgressBar,
        "Size",
        "The size relative to the parent, in scale and pixel offsets.",
    ),
    (
        ClassName::ProgressBar,
        "BackgroundColor3",
        "The background color behind its parts.",
    ),
    (
        ClassName::ProgressBar,
        "BackgroundTransparency",
        "The background opacity: 0 is opaque and 1 is transparent.",
    ),
    (ClassName::ProgressBar, "Visible", "If false, it's hidden."),
    (
        ClassName::ProgressBar,
        "Interactable",
        "If false, the pointer can't change it, it takes no hover, and CSS :disabled styles it. Scripts can still set its Value.",
    ),
    (
        ClassName::ProgressBar,
        "Tooltip",
        "Text shown in a small box near the pointer after a short hover. Empty shows none. Style the box with a CSS Tooltip rule, such as #play Tooltip.",
    ),
    (
        ClassName::ProgressBar,
        "ZIndex",
        "Its place among its siblings: higher is drawn over lower and takes the pointer first, and equal ones keep their order in the tree. CSS `z-index` wins over it. Whole numbers; 1 by default.",
    ),
    (
        ClassName::ProgressBar,
        "Rotation",
        "Degrees clockwise: it's drawn turned around its middle, and clicks hit the turned shape. Layout doesn't change, so its Position and Size are those of the upright box. 0 by default.",
    ),
    (
        ClassName::ProgressBar,
        "Value",
        "How far along it is, from MinValue (empty) to MaxValue (full). Setting it fires ValueChanged.",
    ),
    (
        ClassName::ProgressBar,
        "MinValue",
        "The Value at which it's empty.",
    ),
    (
        ClassName::ProgressBar,
        "MaxValue",
        "The Value at which it's full.",
    ),
    (
        ClassName::ProgressBar,
        "Orientation",
        "\"Horizontal\" (fills from the left) or \"Vertical\" (fills from the bottom). Style the filled part with CSS ProgressBar::fill, which can be a linear-gradient.",
    ),
];

/// What each event means, including what its handler receives. A test
/// checks every event in `core` is here.
pub(super) const EVENT_DOCS: &[(ClassName, &str, &str)] = &[
    (
        ClassName::Part,
        "Touched",
        "Fires when another part starts touching this one, once per new contact (not every frame while they touch). The handler gets the other part: `part.Touched:Connect(function(otherPart) ... end)`.",
    ),
    (
        ClassName::MeshPart,
        "Touched",
        "Fires when another part starts touching this one, once per new contact (not every frame while they touch). The handler gets the other part: `part.Touched:Connect(function(otherPart) ... end)`.",
    ),
    (
        ClassName::RemoteEvent,
        "OnServerEvent",
        "Fires when a LocalScript calls `FireServer`. The handler gets the Player who sent it, then the arguments: `function(player, ...) ... end`.",
    ),
    (
        ClassName::RemoteEvent,
        "OnClientEvent",
        "Fires when a Script calls `FireClient` or `FireAllClients`. The handler gets the arguments.",
    ),
    (
        ClassName::TextButton,
        "Activated",
        "Fires when the player clicks or taps this button. The handler gets no arguments; LocalScripts only.",
    ),
    (
        ClassName::TextBox,
        "FocusLost",
        "Fires when the player finishes typing: Enter on a one-line box, or clicking away (or Escape) on any box. The handler gets no arguments; LocalScripts only.",
    ),
    (
        ClassName::Frame,
        "MouseEnter",
        "Fires when the pointer comes onto this element: the topmost visible, interactable element under it, as CSS :hover follows. The handler gets no arguments; LocalScripts only.",
    ),
    (
        ClassName::Frame,
        "MouseLeave",
        "Fires when the pointer goes off this element (or something else comes on top of it under the pointer). The handler gets no arguments; LocalScripts only.",
    ),
    (
        ClassName::TextLabel,
        "MouseEnter",
        "Fires when the pointer comes onto this element: the topmost visible, interactable element under it, as CSS :hover follows. The handler gets no arguments; LocalScripts only.",
    ),
    (
        ClassName::TextLabel,
        "MouseLeave",
        "Fires when the pointer goes off this element (or something else comes on top of it under the pointer). The handler gets no arguments; LocalScripts only.",
    ),
    (
        ClassName::TextButton,
        "MouseEnter",
        "Fires when the pointer comes onto this element: the topmost visible, interactable element under it, as CSS :hover follows. The handler gets no arguments; LocalScripts only.",
    ),
    (
        ClassName::TextButton,
        "MouseLeave",
        "Fires when the pointer goes off this element (or something else comes on top of it under the pointer). The handler gets no arguments; LocalScripts only.",
    ),
    (
        ClassName::ImageLabel,
        "MouseEnter",
        "Fires when the pointer comes onto this element: the topmost visible, interactable element under it, as CSS :hover follows. The handler gets no arguments; LocalScripts only.",
    ),
    (
        ClassName::ImageLabel,
        "MouseLeave",
        "Fires when the pointer goes off this element (or something else comes on top of it under the pointer). The handler gets no arguments; LocalScripts only.",
    ),
    (
        ClassName::TextBox,
        "MouseEnter",
        "Fires when the pointer comes onto this element: the topmost visible, interactable element under it, as CSS :hover follows. The handler gets no arguments; LocalScripts only.",
    ),
    (
        ClassName::TextBox,
        "MouseLeave",
        "Fires when the pointer goes off this element (or something else comes on top of it under the pointer). The handler gets no arguments; LocalScripts only.",
    ),
    (
        ClassName::PageLayout,
        "MouseEnter",
        "Fires when the pointer comes onto this element: the topmost visible, interactable element under it, as CSS :hover follows. The handler gets no arguments; LocalScripts only.",
    ),
    (
        ClassName::PageLayout,
        "MouseLeave",
        "Fires when the pointer goes off this element (or something else comes on top of it under the pointer). The handler gets no arguments; LocalScripts only.",
    ),
    (
        ClassName::Table,
        "MouseEnter",
        "Fires when the pointer comes onto this element: the topmost visible, interactable element under it, as CSS :hover follows. The handler gets no arguments; LocalScripts only.",
    ),
    (
        ClassName::Table,
        "MouseLeave",
        "Fires when the pointer goes off this element (or something else comes on top of it under the pointer). The handler gets no arguments; LocalScripts only.",
    ),
    (
        ClassName::Sound,
        "Ended",
        "Fires when the sound plays to its end. Looped sounds, and sounds stopped with Stop, don't fire it.",
    ),
    (
        ClassName::Animator,
        "StateChanged",
        "Fires when the animator moves to another state. The handler gets the new state's name, then the old one's: `function(state, previous) ... end`.",
    ),
    (
        ClassName::Animator,
        "AnimationEvent",
        "Fires when a playing clip passes one of its animation events. The handler gets the event's name.",
    ),
    (
        ClassName::NavigationAgent,
        "PlayerSensed",
        "Fires when the agent starts seeing or hearing the player character. The handler gets the sense: \"Sight\" or \"Hearing\".",
    ),
    (
        ClassName::Animation,
        "Completed",
        "Fires when `animation:Play()` reaches the end of a clip that doesn't loop. The handler gets no arguments.",
    ),
    (
        ClassName::Animation,
        "AnimationEvent",
        "Fires when `animation:Play()` passes one of the clip's events. The handler gets the event's name.",
    ),
    (
        ClassName::AnimationTrack,
        "Stopped",
        "Fires when the track has stopped: after Stop's fade, or at the end of a clip that doesn't loop.",
    ),
    (
        ClassName::AnimationTrack,
        "KeyframeReached",
        "Fires each time the track passes one of the clip's events. The handler gets the event's name; GetMarkerReachedSignal hears one name.",
    ),
    (
        ClassName::ScrollingFrame,
        "MouseEnter",
        "Fires when the pointer comes onto this element: the topmost visible, interactable element under it, as CSS :hover follows. The handler gets no arguments; LocalScripts only.",
    ),
    (
        ClassName::ScrollingFrame,
        "MouseLeave",
        "Fires when the pointer goes off this element (or something else comes on top of it under the pointer). The handler gets no arguments; LocalScripts only.",
    ),
    (
        ClassName::Slider,
        "ValueChanged",
        "Fires each time Value changes, from the pointer or a script, with the new Value.",
    ),
    (
        ClassName::Slider,
        "MouseEnter",
        "Fires when the pointer comes onto this element: the topmost visible, interactable element under it, as CSS :hover follows. The handler gets no arguments; LocalScripts only.",
    ),
    (
        ClassName::Slider,
        "MouseLeave",
        "Fires when the pointer goes off this element (or something else comes on top of it under the pointer). The handler gets no arguments; LocalScripts only.",
    ),
    (
        ClassName::Toggle,
        "ValueChanged",
        "Fires each time Value changes, from the pointer or a script, with the new Value.",
    ),
    (
        ClassName::Toggle,
        "MouseEnter",
        "Fires when the pointer comes onto this element: the topmost visible, interactable element under it, as CSS :hover follows. The handler gets no arguments; LocalScripts only.",
    ),
    (
        ClassName::Toggle,
        "MouseLeave",
        "Fires when the pointer goes off this element (or something else comes on top of it under the pointer). The handler gets no arguments; LocalScripts only.",
    ),
    (
        ClassName::Dropdown,
        "ValueChanged",
        "Fires each time Value changes, from the pointer or a script, with the new Value.",
    ),
    (
        ClassName::Dropdown,
        "MouseEnter",
        "Fires when the pointer comes onto this element: the topmost visible, interactable element under it, as CSS :hover follows. The handler gets no arguments; LocalScripts only.",
    ),
    (
        ClassName::Dropdown,
        "MouseLeave",
        "Fires when the pointer goes off this element (or something else comes on top of it under the pointer). The handler gets no arguments; LocalScripts only.",
    ),
    (
        ClassName::ProgressBar,
        "ValueChanged",
        "Fires each time Value changes, from the pointer or a script, with the new Value.",
    ),
    (
        ClassName::ProgressBar,
        "MouseEnter",
        "Fires when the pointer comes onto this element: the topmost visible, interactable element under it, as CSS :hover follows. The handler gets no arguments; LocalScripts only.",
    ),
    (
        ClassName::ProgressBar,
        "MouseLeave",
        "Fires when the pointer goes off this element (or something else comes on top of it under the pointer). The handler gets no arguments; LocalScripts only.",
    ),
];

/// What each class is. A `match` with no `_` case, so adding a class won't
/// compile until it has a description here.
pub(super) fn class_doc(class: ClassName) -> &'static str {
    match class {
        ClassName::DataModel => "The root of the game, called `game` in scripts.",
        ClassName::Workspace => "The service that holds the 3D world. Parts inside it are drawn.",
        ClassName::Players => "The service with a Player for everyone in the game.",
        ClassName::Lighting => {
            "The service for the world's lighting. It sets sun shadows, the sky colors that metal parts reflect, and post-processing (exposure, tone mapping and bloom)."
        }
        ClassName::ReplicatedStorage => {
            "The service for things Scripts and LocalScripts share, such as RemoteEvents and ModuleScripts."
        }
        ClassName::ServerScriptService => {
            "The service that holds server scripts (`scripts/*.server.lua`)."
        }
        ClassName::StarterPlayer => {
            "The service with what each player starts with, such as StarterPlayerScripts."
        }
        ClassName::StarterPlayerScripts => {
            "Holds the LocalScripts (`scripts/*.client.lua`), which run on the player's side."
        }
        ClassName::Part => "A solid shape in the world.",
        ClassName::MeshPart => {
            "A solid 3D model imported from a glTF or FBX file, drawn with the file's own materials (or its SurfaceAppearances'). It collides as the model's convex hull."
        }
        ClassName::Script => "Lua code that runs on the server, from a `.server.lua` file.",
        ClassName::LocalScript => {
            "Lua code that runs on the player's side (input, camera), from a `.client.lua` file."
        }
        ClassName::ModuleScript => {
            "Lua code that scripts load with `require`, from a `.module.lua` file. It runs once per side and returns one value."
        }
        ClassName::RemoteEvent => {
            "Carries messages between Scripts and LocalScripts. Keep it in ReplicatedStorage."
        }
        ClassName::Player => "Someone playing the game, in Players. Made when they join.",
        ClassName::Model => "Groups the parts of one thing, such as a car or a house.",
        ClassName::Folder => "Groups Instances to keep the tree tidy. Has no effect on the game.",
        ClassName::SurfaceAppearance => {
            "Texture maps for a Part, or for one material of a MeshPart (named by MaterialName, in place of the file's own). Delete it to go back to the part's Color and Material, or the file's maps."
        }
        ClassName::ScreenGui => "The on-screen UI root, usually inside Workspace.",
        ClassName::Frame => "A rectangular UI element that can contain other UI elements.",
        ClassName::TextLabel => "A UI element that displays text.",
        ClassName::TextButton => {
            "A clickable UI element. Text and Image can each be empty; both filled shows an icon and a label. Fires Activated when clicked. The old name TextButton still loads."
        }
        ClassName::ImageLabel => {
            "A UI element that displays an image from the project. It is not clickable."
        }
        ClassName::TextBox => {
            "A box the player can type in. MultiLine off (the default): Enter finishes. MultiLine on: Enter is a new line and clicking away finishes. Fires FocusLost."
        }
        ClassName::PageLayout => {
            "Shows one child Frame at a time, chosen by CurrentPage (the Frame's Name). Other children, such as a title, stay visible."
        }
        ClassName::Table => {
            "Lays its children out as equal cells, left to right then top to bottom. Columns is how many cells share a row."
        }
        ClassName::ScrollingFrame => {
            "A Frame that scrolls: its children sit on a canvas (CanvasSize) bigger than the frame, moved by CanvasPosition with the wheel, its scroll bars or a drag. Only what's inside the frame shows and takes clicks."
        }
        ClassName::Slider => {
            "A track with a thumb: drag it, or click the track, to pick a number from MinValue to MaxValue (in Steps). Its parts are drawn, not child Instances, and styled with ::track, ::fill and ::thumb."
        }
        ClassName::Toggle => {
            "An on/off switch or checkbox: a click flips its Value. Its parts are drawn and styled with ::track, ::thumb and ::check, and :checked styles it while it's on."
        }
        ClassName::Dropdown => {
            "A box showing its chosen Value. A click opens a list of its Options, drawn over everything else, which scrolls when long; picking one sets Value, and a click outside closes it. Style the list with ::list and each row with ::option."
        }
        ClassName::ProgressBar => {
            "A bar filled as far as Value is from MinValue to MaxValue, for health, loading and the like. Display only; style the filled part with ::fill."
        }
        ClassName::BillboardGui => {
            "UI that floats in the world, parented to a Part or MeshPart and drawn on the screen UI overlay. Size is on-screen; StudsOffset moves it in world studs."
        }
        ClassName::Sound => {
            "A sound effect or music track that scripts can play. Spatial sounds sit on a part and get quieter with distance from the camera."
        }
        ClassName::PointLight => {
            "A light that shines in all directions from just above its parent part. Inserting one with no part selected also inserts an anchored lamp."
        }
        ClassName::SpotLight => {
            "A light that shines in a cone from just above its parent part. Inserting one with no part selected also inserts an anchored lamp."
        }
        ClassName::ParticleEmitter => {
            "Gives off particles (sparks, smoke, fire, stars...) from inside its parent part, shown in the editor and the game."
        }
        ClassName::Animator => {
            "Inside a Part, MeshPart, Model, ScreenGui or UI element: plays animation on its parent while the game runs: its state machine, if it has one, and the AnimationTracks loaded with LoadAnimation."
        }
        ClassName::Animation => {
            "Points at a clip (`*.anim.json`). Load it with `animator:LoadAnimation(animation)` to play it, or tick AutoPlay to play it when the game starts."
        }
        ClassName::AnimationTrack => {
            "One Animation loaded on an Animator, from `animator:LoadAnimation(animation)`: play, stop and blend it over the state machine."
        }
        ClassName::Terrain => {
            "A landscape: a heightfield shaped in the editor's Terrain Studio, drawn with its TerrainLayers and colliding as its ground. Position is the middle of its footprint at the bottom of its height range; Size is its width, height range and length."
        }
        ClassName::TerrainLayer => {
            "Inside a Terrain: one material it's painted with (a tint, texture maps and tiling), and an optional rule that lays it down by height and slope."
        }
        ClassName::ThirdPersonController => {
            "Walks, runs, jumps and crouches the character model it is parented to, kinematically, up slopes and steps within its limits, and owns the camera during Play: behind the character, zoomed with the mouse wheel. Movement is \"Walk\", \"Run\", \"Crouch\" or a custom mode that scripts move. It is not a physics body: tuning its speeds and limits does not change how other parts fall."
        }
        ClassName::FirstPersonController => {
            "Moves the character model it is parented to just like a ThirdPersonController, with the camera at the character's eyes during Play, looking where the player looks. A character has one controller or the other."
        }
        ClassName::NavigationAgent => {
            "Walks the Model or Part it is parented to during Play: along the navigation mesh to its Destination, round walls and gaps, or straight at its Velocity. It sees and hears the player character within its SightRange, SightAngle and HearingRange, and fires PlayerSensed. Its BehaviorTree, if it has one, decides where it walks. It is not a physics body. View > Overlays > Agent Paths draws the path it is walking."
        }
        ClassName::Checkpoint => {
            "A place in the world to come back to. `SaveService:SetCheckpoint(checkpoint)` makes it the active one and captures the player data and world state there; `SaveService:RestoreCheckpoint()` puts them back and stands the player character at its Position. A save slot keeps the active checkpoint."
        }
    }
}
