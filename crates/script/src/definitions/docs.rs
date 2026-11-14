use super::*;

/// What each property means. A test checks every property in `core` is here.
pub(super) const PROPERTY_DOCS: &[(ClassName, &str, &str)] = &[
    (
        ClassName::Part,
        "Locked",
        "If true, the editor's viewport can't select or change it, and the building tools leave it alone. It has no effect in a running game.",
    ),
    (
        ClassName::MeshPart,
        "Locked",
        "If true, the editor's viewport can't select or change it, and the building tools leave it alone. It has no effect in a running game.",
    ),
    (
        ClassName::Model,
        "Locked",
        "If true, the editor's viewport can't select or change it, and the building tools leave it alone. It has no effect in a running game.",
    ),
    (
        ClassName::Folder,
        "Locked",
        "If true, the editor's viewport can't select or change it, and the building tools leave it alone. It has no effect in a running game.",
    ),
    (
        ClassName::PointLight,
        "Locked",
        "If true, the editor's viewport can't select or change it, and the building tools leave it alone. It has no effect in a running game.",
    ),
    (
        ClassName::SpotLight,
        "Locked",
        "If true, the editor's viewport can't select or change it, and the building tools leave it alone. It has no effect in a running game.",
    ),
    (
        ClassName::RectLight,
        "Locked",
        "If true, the editor's viewport can't select or change it, and the building tools leave it alone. It has no effect in a running game.",
    ),
    (
        ClassName::ParticleEmitter,
        "Locked",
        "If true, the editor's viewport can't select or change it, and the building tools leave it alone. It has no effect in a running game.",
    ),
    (
        ClassName::Part,
        "BlueprintTexture",
        "If true, a plain Plastic part is drawn in a prototype grid texture while editing, to build by. It never shows in Play or in a built game.",
    ),
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
        ClassName::Frame,
        "AccessibleName",
        "The name navigation and scripts use for it, such as one a screen reader would say. Empty uses its Name; reading it from a script gives the name in use.",
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
        ClassName::TextLabel,
        "AccessibleName",
        "The name navigation and scripts use for it, such as one a screen reader would say. Empty uses its Name; reading it from a script gives the name in use.",
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
        ClassName::TextButton,
        "AccessibleName",
        "The name navigation and scripts use for it, such as one a screen reader would say. Empty uses its Name; reading it from a script gives the name in use.",
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
        ClassName::ImageLabel,
        "AccessibleName",
        "The name navigation and scripts use for it, such as one a screen reader would say. Empty uses its Name; reading it from a script gives the name in use.",
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
        ClassName::TextBox,
        "AccessibleName",
        "The name navigation and scripts use for it, such as one a screen reader would say. Empty uses its Name; reading it from a script gives the name in use.",
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
        ClassName::PageLayout,
        "AccessibleName",
        "The name navigation and scripts use for it, such as one a screen reader would say. Empty uses its Name; reading it from a script gives the name in use.",
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
        ClassName::Table,
        "AccessibleName",
        "The name navigation and scripts use for it, such as one a screen reader would say. Empty uses its Name; reading it from a script gives the name in use.",
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
        ClassName::ScrollingFrame,
        "AccessibleName",
        "The name navigation and scripts use for it, such as one a screen reader would say. Empty uses its Name; reading it from a script gives the name in use.",
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
        ClassName::Slider,
        "AccessibleName",
        "The name navigation and scripts use for it, such as one a screen reader would say. Empty uses its Name; reading it from a script gives the name in use.",
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
        ClassName::Toggle,
        "AccessibleName",
        "The name navigation and scripts use for it, such as one a screen reader would say. Empty uses its Name; reading it from a script gives the name in use.",
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
        ClassName::Dropdown,
        "AccessibleName",
        "The name navigation and scripts use for it, such as one a screen reader would say. Empty uses its Name; reading it from a script gives the name in use.",
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
        ClassName::ProgressBar,
        "AccessibleName",
        "The name navigation and scripts use for it, such as one a screen reader would say. Empty uses its Name; reading it from a script gives the name in use.",
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
    (
        ClassName::TextLabel,
        "Text",
        "The text to display. With a TextKey, this is the source text, shown when the string table has no value for it.",
    ),
    (
        ClassName::TextLabel,
        "TextKey",
        "A key in the project's string table, such as \"menu.play\": shows its value in LocalizationService.Language instead of Text. Empty, or a key or language with no value, shows Text.",
    ),
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
    (
        ClassName::TextButton,
        "Text",
        "The text to display. With a TextKey, this is the source text, shown when the string table has no value for it.",
    ),
    (
        ClassName::TextButton,
        "TextKey",
        "A key in the project's string table, such as \"menu.play\": shows its value in LocalizationService.Language instead of Text. Empty, or a key or language with no value, shows Text.",
    ),
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
        ClassName::Sound,
        "RollOffMode",
        "How a spatial sound gets quieter between RollOffMinDistance and RollOffMaxDistance (`Enum.RollOffMode`): Inverse (the default, as sound behaves in the open: half as loud at twice the min distance), Linear, LinearSquare (quieter sooner) or InverseTapered (Inverse, fading out smoothly before the max distance).",
    ),
    (
        ClassName::SoundService,
        "Occlusion",
        "If true (the default), solid parts between a spatial sound and the camera muffle it: each wall, up to three, halves it and takes off more of the highs.",
    ),
    (
        ClassName::Sound,
        "SoundGroup",
        "The SoundGroup it plays through, so that group's Volume and effects (and its parent groups') apply to it. nil plays it straight out.",
    ),
    (
        ClassName::SoundGroup,
        "Volume",
        "A multiplier on everything played through it, as Sound.Volume: 1 leaves it as it is, 0 silences it. A SoundGroup inside another is a sub-bus, so both apply.",
    ),
    (
        ClassName::SoundGroup,
        "UserAdjustable",
        "If true, the default audio settings UI (the AudioSettings ScreenGui) has a volume slider for it.",
    ),
    (
        ClassName::ReverbZone,
        "ReverbType",
        "The room it sounds like (`Enum.ReverbType`): Room (the default), StoneRoom, Hallway, Cave, ConcertHall, Arena or SewerPipe.",
    ),
    (
        ClassName::ReverbZone,
        "FadeTime",
        "Seconds the reverb takes to blend in when the camera comes in, and out when it leaves. 1 by default.",
    ),
    (
        ClassName::GraphicsVolume,
        "Enabled",
        "If false, it changes nothing.",
    ),
    (
        ClassName::GraphicsVolume,
        "BlendDistance",
        "Studs outside its faces over which its look fades in as the camera comes closer, and out as it leaves (5 by default). Inside it, the look is whole. 0 switches it on at the faces.",
    ),
    (
        ClassName::GraphicsVolume,
        "Priority",
        "Where volumes overlap, the one with the higher Priority wins (0 by default). Equal ones follow Explorer order, the later winning.",
    ),
    (
        ClassName::ExposureEffect,
        "Enabled",
        "If false, it changes nothing.",
    ),
    (
        ClassName::ExposureEffect,
        "Exposure",
        "Multiplies the scene's brightness before tone mapping, in place of Lighting's Exposure (1 by default).",
    ),
    (
        ClassName::FogEffect,
        "Enabled",
        "If false, it changes nothing.",
    ),
    (
        ClassName::FogEffect,
        "Color",
        "The fog's color. Light grey by default.",
    ),
    (
        ClassName::FogEffect,
        "Start",
        "Studs from the camera where the fog begins (0 by default).",
    ),
    (
        ClassName::FogEffect,
        "End",
        "Studs from the camera where everything is the fog's color (200 by default).",
    ),
    (
        ClassName::FogEffect,
        "Density",
        "Above 0, the fog is dust or mist filling the volume's box, seen from anywhere, instead of distance fog: it takes away this much of the light per stud (0.02 hides about half of what's 35 studs away), and Color is how much of the light it takes it scatters back. It's lit by the sun and its shadows (shafts of light, with volumetric fog on), by lamps and by the light around it (light probes), and it thins to nothing over BlendDistance at the box's faces. 0 by default.",
    ),
    (
        ClassName::FogEffect,
        "Glare",
        "How much of the dust's light goes forward, away from the sun, from 0 to 10 (0 by default): more makes it glow brighter looking toward the sun.",
    ),
    (
        ClassName::FogEffect,
        "HeightFalloff",
        "Studs above the volume's bottom over which its dust thins by e, for haze lying low in a valley; 0 (the default) keeps it even all the way up.",
    ),
    (
        ClassName::FogEffect,
        "Interior",
        "Makes the volume an interior (a room, a building): inside its box the air outside is cleared (an Atmosphere's fog, Lighting's fog and lower volumes' dust), and only this effect's own Density fills it, 0 for clear air. False by default.",
    ),
    (
        ClassName::BloomEffect,
        "Enabled",
        "If false, it changes nothing.",
    ),
    (
        ClassName::BloomEffect,
        "Intensity",
        "How much of the bloom is added to the picture, in place of Lighting's BloomIntensity (0.35 by default). 0 turns bloom off there.",
    ),
    (
        ClassName::BloomEffect,
        "Threshold",
        "Only light brighter than this blooms, in place of Lighting's BloomThreshold (1 by default).",
    ),
    (
        ClassName::ReverbSoundEffect,
        "Enabled",
        "If false, the sound passes this effect by as if it weren't there. Changes apply while it plays.",
    ),
    (
        ClassName::ReverbSoundEffect,
        "Priority",
        "Effects with a higher Priority are applied first; equal ones go in their order among the children. 0 by default.",
    ),
    (
        ClassName::ReverbSoundEffect,
        "DecayTime",
        "Seconds the reverb's tail takes to die away, from 0.1 to 20. 1.5 by default.",
    ),
    (
        ClassName::ReverbSoundEffect,
        "Density",
        "From 0 to 1: lower is darker and sparser, 1 (the default) a bright, smooth wash.",
    ),
    (
        ClassName::ReverbSoundEffect,
        "DryLevel",
        "Decibels of the sound as it is, from -80 to 20. -6 by default.",
    ),
    (
        ClassName::ReverbSoundEffect,
        "WetLevel",
        "Decibels of the reverb, from -80 to 20. 0 by default.",
    ),
    (
        ClassName::EqualizerSoundEffect,
        "Enabled",
        "If false, the sound passes this effect by as if it weren't there. Changes apply while it plays.",
    ),
    (
        ClassName::EqualizerSoundEffect,
        "Priority",
        "Effects with a higher Priority are applied first; equal ones go in their order among the children. 0 by default.",
    ),
    (
        ClassName::EqualizerSoundEffect,
        "LowGain",
        "Decibels added below MidRange's X (negative turns them down), from -80 to 10.",
    ),
    (
        ClassName::EqualizerSoundEffect,
        "MidGain",
        "Decibels added between MidRange's X and Y, from -80 to 10.",
    ),
    (
        ClassName::EqualizerSoundEffect,
        "HighGain",
        "Decibels added above MidRange's Y, from -80 to 10.",
    ),
    (
        ClassName::EqualizerSoundEffect,
        "MidRange",
        "The mids, in hertz: from X to Y. 400 to 4000 by default.",
    ),
    (
        ClassName::CompressorSoundEffect,
        "Enabled",
        "If false, the sound passes this effect by as if it weren't there. Changes apply while it plays.",
    ),
    (
        ClassName::CompressorSoundEffect,
        "Priority",
        "Effects with a higher Priority are applied first; equal ones go in their order among the children. 0 by default.",
    ),
    (
        ClassName::CompressorSoundEffect,
        "Threshold",
        "Decibels, from -60 to 0, above which the sound is turned down. -40 by default.",
    ),
    (
        ClassName::CompressorSoundEffect,
        "Ratio",
        "How much it's turned down above Threshold, from 1 (not at all) to 50: 4 lets through a quarter. 40 by default.",
    ),
    (
        ClassName::CompressorSoundEffect,
        "Attack",
        "Seconds it takes to turn the sound down once it's over Threshold. 0.1 by default.",
    ),
    (
        ClassName::CompressorSoundEffect,
        "Release",
        "Seconds it takes to turn back up once it's under. 0.1 by default.",
    ),
    (
        ClassName::CompressorSoundEffect,
        "GainMakeup",
        "Decibels, from 0 to 30, everything is turned back up by after.",
    ),
    (
        ClassName::EchoSoundEffect,
        "Enabled",
        "If false, the sound passes this effect by as if it weren't there. Changes apply while it plays.",
    ),
    (
        ClassName::EchoSoundEffect,
        "Priority",
        "Effects with a higher Priority are applied first; equal ones go in their order among the children. 0 by default.",
    ),
    (
        ClassName::EchoSoundEffect,
        "Delay",
        "Seconds between repeats, from 0.01 to 5. 1 by default.",
    ),
    (
        ClassName::EchoSoundEffect,
        "Feedback",
        "Decibels each repeat is quieter than the one before, from -80 to 0. -3 by default.",
    ),
    (
        ClassName::EchoSoundEffect,
        "DryLevel",
        "Decibels of the sound as it is, from -80 to 20.",
    ),
    (
        ClassName::EchoSoundEffect,
        "WetLevel",
        "Decibels of the repeats, from -80 to 20.",
    ),
    (
        ClassName::DistortionSoundEffect,
        "Enabled",
        "If false, the sound passes this effect by as if it weren't there. Changes apply while it plays.",
    ),
    (
        ClassName::DistortionSoundEffect,
        "Priority",
        "Effects with a higher Priority are applied first; equal ones go in their order among the children. 0 by default.",
    ),
    (
        ClassName::DistortionSoundEffect,
        "Level",
        "How much it's overdriven, from 0 (clean) to 1 (fuzz). 0.75 by default.",
    ),
    (
        ClassName::PitchShiftSoundEffect,
        "Enabled",
        "If false, the sound passes this effect by as if it weren't there. Changes apply while it plays.",
    ),
    (
        ClassName::PitchShiftSoundEffect,
        "Priority",
        "Effects with a higher Priority are applied first; equal ones go in their order among the children. 0 by default.",
    ),
    (
        ClassName::PitchShiftSoundEffect,
        "Octave",
        "How much higher it sounds, from 0.5 (an octave down) to 2 (an octave up), at the same speed. 1.25 by default.",
    ),
    (
        ClassName::LowPassSoundEffect,
        "Enabled",
        "If false, the sound passes this effect by as if it weren't there. Changes apply while it plays.",
    ),
    (
        ClassName::LowPassSoundEffect,
        "Priority",
        "Effects with a higher Priority are applied first; equal ones go in their order among the children. 0 by default.",
    ),
    (
        ClassName::LowPassSoundEffect,
        "Cutoff",
        "Hertz above which the sound fades away: lower is more muffled. 1000 by default.",
    ),
    (
        ClassName::LowPassSoundEffect,
        "Resonance",
        "From 0 to 1: how much it rings at Cutoff. 0 by default.",
    ),
    (
        ClassName::HighPassSoundEffect,
        "Enabled",
        "If false, the sound passes this effect by as if it weren't there. Changes apply while it plays.",
    ),
    (
        ClassName::HighPassSoundEffect,
        "Priority",
        "Effects with a higher Priority are applied first; equal ones go in their order among the children. 0 by default.",
    ),
    (
        ClassName::HighPassSoundEffect,
        "Cutoff",
        "Hertz below which the sound fades away: higher is thinner. 500 by default.",
    ),
    (
        ClassName::HighPassSoundEffect,
        "Resonance",
        "From 0 to 1: how much it rings at Cutoff. 0 by default.",
    ),
    (
        ClassName::Workspace,
        "Gravity",
        "How hard unanchored parts and the characters are pulled down, in studs a second squared: 196.2 by default. Halve it and things fall slower and characters jump higher.",
    ),
    (
        ClassName::Workspace,
        "MaxSpawnedDecals",
        "The most decals `workspace:SpawnDecal` keeps at once on each side: 200 by default. Past it, the oldest goes first.",
    ),
    (
        ClassName::Workspace,
        "PhysicsInterpolation",
        "On (the default), moving parts are drawn smoothly between physics steps, which run 60 times a second, so they don't move in steps on faster screens. What's drawn trails the simulation by under a step; scripts, touches and the network still see the stepped values. Off, parts are drawn where the last step left them.",
    ),
    (
        ClassName::Workspace,
        "StreamingEnabled",
        "On, each device only has what's near it (world streaming): joined players' devices are sent only what's within their radius, and the rest stays off the GPU. Off by default, so a project plays exactly as before.",
    ),
    (
        ClassName::Workspace,
        "StreamingMinRadius",
        "Everything nearer than this many studs is always loaded: 64 by default.",
    ),
    (
        ClassName::Workspace,
        "StreamingTargetRadius",
        "Things load out to this many studs, as the network and the machine allow, and unload a bit past it: 1024 by default. Never less than StreamingMinRadius.",
    ),
    (
        ClassName::Model,
        "ModelStreamingMode",
        "How the Model streams. Default: a Model with no Models inside loads and unloads whole, and one with Models inside streams each piece on its own. Atomic: always whole, with the Models inside. Persistent: always on every device.",
    ),
    (
        ClassName::Decal,
        "Texture",
        "The picture, a project path such as \"assets/textures/logo.png\". Its alpha is its shape. Empty shows a plain square.",
    ),
    (
        ClassName::Decal,
        "Face",
        "The face of its part it's on: `Enum.NormalId.Front` (the default, -Z), `Back`, `Left`, `Right`, `Top` or `Bottom`.",
    ),
    (
        ClassName::Decal,
        "Color3",
        "Tints the picture: white shows it as it is.",
    ),
    (
        ClassName::Decal,
        "Transparency",
        "0 shows the picture as it is, 1 hides it.",
    ),
    (
        ClassName::Decal,
        "StudsPerTileU",
        "Studs one copy of the picture covers across the face; it repeats to fill it. 0 (the default) stretches one copy across.",
    ),
    (
        ClassName::Decal,
        "StudsPerTileV",
        "Studs one copy of the picture covers down the face; it repeats to fill it. 0 (the default) stretches one copy down.",
    ),
    (
        ClassName::ProjectedDecal,
        "Texture",
        "The picture, a project path such as \"assets/textures/hole.png\". Its alpha is its shape. Empty shows a plain square.",
    ),
    (
        ClassName::ProjectedDecal,
        "Color3",
        "Tints the picture: white shows it as it is.",
    ),
    (
        ClassName::ProjectedDecal,
        "Transparency",
        "0 shows the picture as it is, 1 hides it. `SpawnDecal`'s decals fade by raising it.",
    ),
    (
        ClassName::ProjectedDecal,
        "Position",
        "The middle of its box, in studs.",
    ),
    (
        ClassName::ProjectedDecal,
        "Orientation",
        "Its turn in degrees (Z, then X, then Y). The picture faces the box's front, -Z (its CFrame's LookVector); the default, 90 around X, faces up, so it lies on the ground below.",
    ),
    (
        ClassName::ProjectedDecal,
        "Size",
        "The picture's width and height, then how deep the box reaches (half in front of its middle, half behind): only surfaces inside the box are painted.",
    ),
    (
        ClassName::ProjectedDecal,
        "FadeAngle",
        "Degrees a surface may turn away from the picture's facing before the decal is gone from it: 75 by default. It fades over the second half of that turn, so the sides of what's in the box aren't smeared.",
    ),
    (
        ClassName::ProjectedDecal,
        "IgnoreCharacters",
        "On (the default), characters walking through the box stay bare: it paints only the world around them.",
    ),
    (
        ClassName::Workspace,
        "GlobalWind",
        "The wind everywhere, in studs a second. It carries particles from emitters with WindAffectsDrag, pushes unanchored parts with WindAffected, carries Lighting's Clouds along (its level part), and blows the Weather's rain and snow.",
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
        "SkyImage",
        "A picture of the whole sky, such as \"assets/sky/noon.hdr\", shown behind the world and reflected by parts in place of the sky colors, with its light on everything. A Radiance .hdr file keeps the sun brighter than white; a PNG or JPEG also works. It wraps once around the world, its middle toward -Z and top straight up. Empty for the sky colors. Only drawn where the player's graphics settings have reflection quality above Low.",
    ),
    (
        ClassName::Lighting,
        "FogColor",
        "The color far things fade into with distance fog (FogStart to FogEnd). Not used while Lighting has an Atmosphere, whose fog takes the sky's color.",
    ),
    (
        ClassName::Lighting,
        "FogStart",
        "How many studs from the camera distance fog starts. Nearer is clear.",
    ),
    (
        ClassName::Lighting,
        "FogEnd",
        "How many studs from the camera everything is FogColor, including the sky. The default, 100000, is further than anything is seen, so there's no fog. An Atmosphere's fog takes its place.",
    ),
    (
        ClassName::Lighting,
        "ClockTime",
        "The time of day in hours, from 0 to 24 (25 becomes 1). The sun rises due east at 6, is highest at 12 and sets due west at 18; the moon, opposite it, lights the night, when the stars come out. Worlds made before it existed start at about 10.27, where their sun always was.",
    ),
    (
        ClassName::Lighting,
        "GeographicLatitude",
        "How far north of the equator the world is, in degrees (negative is south). The noon sun stands this far from straight up, toward +Z north of the equator.",
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
        ClassName::PointLight,
        "Shadows",
        "If true, parts around the light cast shadows on every side of it. Only the few point lights nearest the camera do, as the graphics settings allow.",
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
        "The way the beam points, relative to the parent part (or to its own Orientation when it isn't in a part).",
    ),
    (
        ClassName::SpotLight,
        "Shadows",
        "If true, parts in the beam cast shadows. Only the few spot lights nearest the camera do, as the graphics settings allow.",
    ),
    (
        ClassName::PointLight,
        "Position",
        "Where it is in the world when it isn't inside a part, in studs. Inside a part it works from the part and this isn't used.",
    ),
    (
        ClassName::PointLight,
        "Orientation",
        "How it's turned when it isn't inside a part, in degrees, as a part's Orientation. Inside a part it turns with the part.",
    ),
    (
        ClassName::SpotLight,
        "Position",
        "Where it is in the world when it isn't inside a part, in studs. Inside a part it works from the part and this isn't used.",
    ),
    (
        ClassName::SpotLight,
        "Orientation",
        "How it's turned when it isn't inside a part, in degrees, as a part's Orientation. Inside a part it turns with the part.",
    ),
    (
        ClassName::RectLight,
        "Position",
        "Where it is in the world when it isn't inside a part, in studs. Inside a part it works from the part and this isn't used.",
    ),
    (
        ClassName::RectLight,
        "Orientation",
        "How it's turned when it isn't inside a part, in degrees, as a part's Orientation. Inside a part it turns with the part.",
    ),
    (
        ClassName::ParticleEmitter,
        "Position",
        "Where it is in the world when it isn't inside a part, in studs. Inside a part it works from the part and this isn't used.",
    ),
    (
        ClassName::ParticleEmitter,
        "Orientation",
        "How it's turned when it isn't inside a part, in degrees, as a part's Orientation. Inside a part it turns with the part.",
    ),
    (
        ClassName::RectLight,
        "Enabled",
        "If false, the light is off.",
    ),
    (ClassName::RectLight, "Color", "The light's color."),
    (
        ClassName::RectLight,
        "Brightness",
        "How bright the light is.",
    ),
    (
        ClassName::RectLight,
        "Range",
        "How far the light reaches, in studs.",
    ),
    (
        ClassName::RectLight,
        "Width",
        "How wide the rectangle the light shines from is, in studs.",
    ),
    (
        ClassName::RectLight,
        "Height",
        "How tall the rectangle the light shines from is, in studs.",
    ),
    (
        ClassName::RectLight,
        "Direction",
        "The way the rectangle's front faces, relative to the parent part (or to its own Orientation when it isn't in a part).",
    ),
    (
        ClassName::RectLight,
        "Shadows",
        "If true, parts in front of it cast shadows. Only the few lights nearest the camera do, as the graphics settings allow.",
    ),
    (
        ClassName::ReflectionProbe,
        "Bake",
        "The bake the editor saved for it (Lighting > Bake Probes), such as \"assets/probes/Hall.probe.hdr\". Empty, or a file that's missing, reflects the sky instead.",
    ),
    (
        ClassName::ReflectionProbe,
        "BlendDistance",
        "How far inside the part's faces, in studs, its reflection fades in from what's around it.",
    ),
    (
        ClassName::LightProbeVolume,
        "Bake",
        "The bake the editor saved for it (Lighting > Bake Probes), such as \"assets/probes/Hall.lightprobes.json\". Empty, or a file that's missing, leaves the sky's light instead.",
    ),
    (
        ClassName::LightProbeVolume,
        "Spacing",
        "About how many studs apart its probes stand along each side of the part (at most 16 probes a side). Bake again after changing it.",
    ),
    (
        ClassName::LightProbeVolume,
        "BlendDistance",
        "How far inside the part's faces, in studs, its light fades in from what's around it.",
    ),
    (
        ClassName::Atmosphere,
        "Density",
        "How thick the air is, from 0 (none: a black sky and a white sun, and no fog) to 1. 0.395 is Earth's: its fog hides half of what's 500 studs away at ground level.",
    ),
    (
        ClassName::Atmosphere,
        "Offset",
        "How high above the ground the sky is seen from, from 0 to 1 (one height of the air up, where the sky above darkens). The fog is thickest at the bottom of the air and thins with height, every 50 studs by about a third; Offset lifts the world above that bottom (by 250 studs at 1), so the fog thins.",
    ),
    (
        ClassName::Atmosphere,
        "Color",
        "Tints the whole sky. The default grey leaves it as the air scatters it.",
    ),
    (
        ClassName::Atmosphere,
        "Decay",
        "Tints the sky away from the sun. The default blue-grey leaves it as the air scatters it.",
    ),
    (
        ClassName::Atmosphere,
        "Glare",
        "Brightens the glow around the sun, from 0 to 10, and tightens its glow in the fog.",
    ),
    (
        ClassName::Atmosphere,
        "Haze",
        "Adds dust and haze to the air, from 0 to 10: a paler horizon, a wider glow around the sun, a dimmer, redder sun, and thicker fog (1 + Haze times as thick).",
    ),
    (
        ClassName::Clouds,
        "Cover",
        "How much of the sky is cloud, from 0 (a clear sky) to 1 (overcast).",
    ),
    (
        ClassName::Clouds,
        "Density",
        "How thick the clouds are, from 0 (none) to 1: thicker clouds are whiter on top, darker underneath, and cast deeper shadows.",
    ),
    (
        ClassName::Clouds,
        "Color",
        "The clouds' own color, which the sun and sky light. White by default.",
    ),
    (
        ClassName::Clouds,
        "Speed",
        "How fast the clouds drift on their own, in studs a second (20 by default; 0 to hold them still). workspace.GlobalWind adds to it. Their shadows move with them.",
    ),
    (
        ClassName::Clouds,
        "Direction",
        "Which way the clouds drift on their own, in degrees turned from +X (east, 0) toward +Z (south, 90).",
    ),
    (
        ClassName::OutlineEffect,
        "Enabled",
        "Whether the lines are drawn. Off (or with no OutlineEffect in Lighting) they cost nothing.",
    ),
    (
        ClassName::OutlineEffect,
        "Color",
        "The lines' color. Black by default.",
    ),
    (
        ClassName::OutlineEffect,
        "Thickness",
        "How thick the lines are, in pixels of a view 1080 pixels tall (1 by default), scaled with the view's height and never under one pixel.",
    ),
    (
        ClassName::ColorCorrectionEffect,
        "Enabled",
        "Whether it changes the picture. Off (or with no ColorCorrectionEffect in Lighting) it costs nothing.",
    ),
    (
        ClassName::ColorCorrectionEffect,
        "Brightness",
        "Added to every color channel, from -1 (black) to 1 (white). 0 by default.",
    ),
    (
        ClassName::ColorCorrectionEffect,
        "Contrast",
        "Above 0 spreads colors away from mid-grey; below it squeezes them toward it, down to -1 (flat grey). 0 by default.",
    ),
    (
        ClassName::ColorCorrectionEffect,
        "Saturation",
        "Above 0 makes colors more vivid; below it greys them, down to -1 (black and white). 0 by default.",
    ),
    (
        ClassName::ColorCorrectionEffect,
        "TintColor",
        "Multiplies every color. White (the default) changes nothing.",
    ),
    (
        ClassName::ColorCorrectionEffect,
        "LookupTable",
        "A lookup table in the project every color is looked up in: a .cube file, or a strip image of N squares of N × N pixels side by side (such as 256 × 16). \"\" for none.",
    ),
    (
        ClassName::ColorCorrectionEffect,
        "Posterize",
        "How many levels each color channel is cut down to, 2 or more, for a flat, poster look. 0 (the default) leaves them smooth.",
    ),
    (
        ClassName::ColorCorrectionEffect,
        "Palette",
        "An image in the project whose colors (up to 64) every pixel is snapped to the nearest of, for a retro look. \"\" for none.",
    ),
    (
        ClassName::DepthOfFieldEffect,
        "Enabled",
        "Whether it blurs anything. Off (or with no DepthOfFieldEffect in Lighting) it costs nothing.",
    ),
    (
        ClassName::DepthOfFieldEffect,
        "FocusDistance",
        "How far from the camera, in studs, the middle of what's sharp is (20 by default).",
    ),
    (
        ClassName::DepthOfFieldEffect,
        "InFocusRadius",
        "How far either side of FocusDistance stays sharp, in studs (10 by default).",
    ),
    (
        ClassName::DepthOfFieldEffect,
        "NearIntensity",
        "How blurred what's nearer than the sharp range gets, from 0 (not at all) to 1, growing toward the camera. 0.75 by default.",
    ),
    (
        ClassName::DepthOfFieldEffect,
        "FarIntensity",
        "How blurred what's farther than the sharp range gets, from 0 (not at all) to 1, reached at twice the sharp range's far edge, the sky included. 0.75 by default.",
    ),
    (
        ClassName::VignetteEffect,
        "Enabled",
        "Whether it's drawn. Off (or with no VignetteEffect in Lighting) it costs nothing.",
    ),
    (
        ClassName::VignetteEffect,
        "Intensity",
        "How much of Color the corners take, from 0 to 1 (0.5 by default).",
    ),
    (
        ClassName::VignetteEffect,
        "Size",
        "Where the darkening starts, from the middle (0) to the corners (1). 0.5 by default.",
    ),
    (
        ClassName::VignetteEffect,
        "Color",
        "What the corners turn toward. Black by default.",
    ),
    (
        ClassName::FilmGrainEffect,
        "Enabled",
        "Whether it's drawn. Off (or with no FilmGrainEffect in Lighting) it costs nothing.",
    ),
    (
        ClassName::FilmGrainEffect,
        "Intensity",
        "How strong the noise is, from 0 to 1 (0.3 by default).",
    ),
    (
        ClassName::FilmGrainEffect,
        "Size",
        "Each grain's size, in pixels of a view 1080 pixels tall (1 by default), scaled with the view's height and never under one pixel.",
    ),
    (
        ClassName::ChromaticAberrationEffect,
        "Enabled",
        "Whether it's drawn. Off (or with no ChromaticAberrationEffect in Lighting) it costs nothing.",
    ),
    (
        ClassName::ChromaticAberrationEffect,
        "Intensity",
        "How far red and blue drift apart toward the picture's edges: 1 parts them by 1% of its width at the left and right edges, and nothing in the middle. 0.5 by default.",
    ),
    (
        ClassName::SharpenEffect,
        "Enabled",
        "Whether it's drawn. Off (or with no SharpenEffect in Lighting) it costs nothing.",
    ),
    (
        ClassName::SharpenEffect,
        "Intensity",
        "How much crisper the edges between colors get, from 0 (no change) to 1 (strong). 0.5 by default.",
    ),
    (
        ClassName::SunRaysEffect,
        "Enabled",
        "Whether it's drawn. Off (or with no SunRaysEffect in Lighting) it costs nothing.",
    ),
    (
        ClassName::SunRaysEffect,
        "Intensity",
        "How bright the shafts are, from 0 (none) to 1 (strong). 0.25 by default.",
    ),
    (
        ClassName::SunRaysEffect,
        "Spread",
        "How far the shafts reach from the sun across the picture, from 0 (a glow around it) to 1 (the whole way across). 0.5 by default.",
    ),
    (
        ClassName::MotionBlurEffect,
        "Enabled",
        "Whether it's drawn. Off (or with no MotionBlurEffect in Lighting) it costs nothing.",
    ),
    (
        ClassName::MotionBlurEffect,
        "Intensity",
        "How much of the way something moved across the view in a frame it streaks over, from 0 (none) to 1 (all of it, a shutter open the whole frame). 0.5 by default. Streaks are at most 1/18 of the view's height long.",
    ),
    (
        ClassName::Weather,
        "Precipitation",
        "What falls around the camera (`Enum.Precipitation`): None, Rain (the default) or Snow. Rain wets surfaces as it falls, up to its Intensity; snow covers the ones facing up.",
    ),
    (
        ClassName::Weather,
        "Intensity",
        "How hard the rain or snow falls, from 0 to 1 (0.5 by default): more drops, and wetter or deeper on the ground.",
    ),
    (
        ClassName::Weather,
        "Wetness",
        "How wet surfaces are whatever is falling, from 0 (dry, the default) to 1 (soaked): darker and glossier. Rain wets them at least as much as its Intensity.",
    ),
    (
        ClassName::Weather,
        "SnowCover",
        "How much snow lies on surfaces facing up whatever is falling, from 0 (none, the default) to 1: more cover whitens flat tops fully and reaches further down slopes. Walls stay bare. Falling snow covers them at least as much as its Intensity.",
    ),
    (
        ClassName::Weather,
        "TransitionTime",
        "Seconds a change takes to ease in (10 by default): rain starting and stopping, surfaces wetting and drying, snow settling and melting. 0 changes at once.",
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
        ClassName::Sequence,
        "SequenceId",
        "The sequence file inside the project folder, such as \"assets/sequences/Intro.sequence.json\".",
    ),
    (
        ClassName::Sequence,
        "AutoPlay",
        "If true, it plays when the game starts.",
    ),
    (
        ClassName::Sequence,
        "Skippable",
        "If true, the Menu action (Escape, or Start on a controller) jumps it to its end while it plays: what it keys is left at its last values and Completed fires once.",
    ),
    (
        ClassName::Sequence,
        "LockPlayerInput",
        "If true, the character doesn't move and scripts and prompts get no presses from the player while it plays or is paused. Control comes back when it ends or stops.",
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
        "SizeOverLife",
        "A NumberSequence of studs across over each particle's life, from birth (0) to death (1), in place of Size and SizeEnd. nil (the default) uses them.",
    ),
    (
        ClassName::ParticleEmitter,
        "ColorOverLife",
        "A ColorSequence over each particle's life, in place of Color and ColorEnd. nil (the default) uses them.",
    ),
    (
        ClassName::ParticleEmitter,
        "TransparencyOverLife",
        "A NumberSequence from 0 (solid) to 1 (invisible) over each particle's life, in place of Transparency and TransparencyEnd. nil (the default) uses them.",
    ),
    (
        ClassName::ParticleEmitter,
        "SpeedOverLife",
        "A NumberSequence that multiplies Speed over each particle's life, from birth (0) to death (1): how far it moves each moment, with Acceleration and Drag still acting on its velocity. Envelopes aren't used. nil (the default) keeps Speed.",
    ),
    (
        ClassName::ParticleEmitter,
        "Shape",
        "Where in the part particles are born: `Enum.ParticleEmitterShape.Box` (the default, the whole part), `Sphere` (the ball inside it), `Cylinder` (upright along the part's Y) or `Disc` (flat across its middle, facing its Y). A Box's particles head along Direction within SpreadAngle; the round shapes' head away from their middle (see ShapeInOut).",
    ),
    (
        ClassName::ParticleEmitter,
        "ShapeStyle",
        "`Enum.ParticleEmitterShapeStyle.Volume` (the default) gives off particles anywhere inside the Shape; `Surface` only on it: a box's faces, the sphere, a cylinder's round side or a disc's rim.",
    ),
    (
        ClassName::ParticleEmitter,
        "ShapeInOut",
        "Which way particles head from the Shape, within SpreadAngle: `Enum.ParticleEmitterShapeInOut.Outward` (the default: along Direction for a Box; away from the middle for a Sphere, and from the upright axis for a Cylinder or Disc), `Inward` (the other way) or `InAndOut` (either, at random).",
    ),
    (
        ClassName::ParticleEmitter,
        "VelocityInheritance",
        "How much of the part's own velocity each particle starts with, from 0 (none, the default) to 1 (all of it), so sparks from a moving car keep up with it.",
    ),
    (
        ClassName::ParticleEmitter,
        "Collides",
        "On, particles bounce off parts with CanCollide on and the ground (not off the emitter's own part, nor parts with CanCollide off), and come to rest on a floor. A big emitter moving its particles on the GPU lands them only on what the camera sees. Default false.",
    ),
    (
        ClassName::ParticleEmitter,
        "Bounce",
        "How much of a colliding particle's speed into a surface it keeps, back out of it: 0 lands dead, 1 bounces as high as it fell. Speed along the surface is kept. Default 0.5.",
    ),
    (
        ClassName::ParticleEmitter,
        "LifetimeLoss",
        "How much of its life a colliding particle loses each time it lands, from 0 (none, the default) to 1 (it dies where it lands).",
    ),
    (
        ClassName::ParticleEmitter,
        "EmitOn",
        "Inside another ParticleEmitter, `Enum.ParticleEmitterEmitOn.Birth`, `Death` or `Collision` makes this one a sub-emitter: while Enabled it gives off EmitCount particles where each of its parent's particles is born, dies or lands, heading out along its own Direction (turned by its Orientation), and nothing by its Rate. `Always` (the default) gives off as any emitter does.",
    ),
    (
        ClassName::ParticleEmitter,
        "EmitCount",
        "How many particles a sub-emitter (EmitOn Birth, Death or Collision) gives off at each of its parent's events. Default 10.",
    ),
    (
        ClassName::ParticleEmitter,
        "Bursts",
        "A NumberSequence of bursts while Enabled: each keypoint gives off Value particles (give or take Envelope) at its Time of every Duration seconds, over and over. Rate still adds its steady flow. A keypoint at time 1 is the next loop's start, so it gives off nothing: put that count at time 0. nil (the default) is no bursts.",
    ),
    (
        ClassName::ParticleEmitter,
        "Duration",
        "How long one loop of Bursts takes, in seconds (default 1, at least 0.05).",
    ),
    (
        ClassName::ParticleEmitter,
        "Rotation",
        "How far each particle is turned when it's born, in degrees.",
    ),
    (
        ClassName::ParticleEmitter,
        "FlipbookLayout",
        "Cuts the Texture into a grid of frames that each particle shows one at a time, read left to right, then top to bottom: `Enum.ParticleFlipbookLayout.Grid2x2`, `Grid4x4` or `Grid8x8` (4, 16 or 64 frames). `None` (the default) shows the whole Texture. The built-in ParticleType pictures have no frames, so it needs a Texture.",
    ),
    (
        ClassName::ParticleEmitter,
        "FlipbookMode",
        "Which frame a particle shows as it ages: `Enum.ParticleFlipbookMode.Loop` (the default: in order at FlipbookFramerate, then from the first again), `OneShot` (every frame once over its life, whatever the frame rate), `PingPong` (in order, then back) or `Random` (a frame at random, again at the frame rate).",
    ),
    (
        ClassName::ParticleEmitter,
        "FlipbookFramerate",
        "How many flipbook frames a second each particle moves on (default 1). OneShot ignores it.",
    ),
    (
        ClassName::ParticleEmitter,
        "FlipbookStartRandom",
        "When true, each particle starts on a frame of its own instead of the first (Loop and PingPong), so a puff of smoke doesn't animate in step.",
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
        "Block, Ball, Cylinder (along X), Wedge, Capsule (along X, with round ends) or CornerWedge (rising to a point over its back right corner).",
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
        "CollisionGroup",
        "The collision group it's in, `\"Default\"` at first. Parts in two groups set not to collide (the Collision Groups window, or `PhysicsService:CollisionGroupSetCollidable`) pass through each other and don't touch. A name that isn't registered counts as Default.",
    ),
    (
        ClassName::Part,
        "CanQuery",
        "If false, scripts' raycasts, shape casts and overlap queries (`workspace:Raycast`, `GetPartsInPart`, ...) pass it by. It still collides.",
    ),
    (
        ClassName::Part,
        "CanTouch",
        "If false, it fires no Touched or TouchEnded, and touching it fires neither on other parts. It still collides.",
    ),
    (
        ClassName::Part,
        "LinearDrag",
        "Air resistance on its speed: each second its assembly loses about that share of it (1 takes away about 63%). 0 at first. Welded parts use their root part's.",
    ),
    (
        ClassName::Part,
        "AngularDrag",
        "Air resistance on its spin, like LinearDrag. 0.5 at first, so a spinning part slows by itself; 0 spins on.",
    ),
    (
        ClassName::Part,
        "RollingFriction",
        "Extra spin slowing while its assembly touches anything, so a ball rolls to a stop on the ground but spins on in the air. 0 at first. Welded parts use their root part's.",
    ),
    (
        ClassName::Part,
        "Massless",
        "If true, the part adds no mass to an assembly it shares with other parts. A part on its own (every part, until welds exist) keeps its own mass.",
    ),
    (
        ClassName::Part,
        "CustomPhysicalProperties",
        "Its own density, friction, elasticity and their weights (`PhysicalProperties.new(...)`), or nil for its Material's. Read `CurrentPhysicalProperties` for the ones in use.",
    ),
    (
        ClassName::Part,
        "AssemblyLinearVelocity",
        "How fast the part moves, in studs a second. Physics updates it as the part moves; setting it throws the part. Anchored parts don't move.",
    ),
    (
        ClassName::Part,
        "AssemblyAngularVelocity",
        "How fast the part spins, in radians a second around each world axis. Physics updates it; setting it spins the part.",
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
        ClassName::UnionOperation,
        "UsePartColor",
        "If true, the whole solid is drawn in Color. If false (the default), each face keeps the color and material of the part it came from.",
    ),
    (
        ClassName::UnionOperation,
        "Color",
        "The color the whole solid is drawn in while UsePartColor is on. Union sets it to the first part's.",
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
        "CollisionFidelity",
        "What it collides as: `Enum.CollisionFidelity.Default` (a MeshPart's default: the model's own collision pieces, its objects named `<name>_COL`, when it has any, else its hull), `Box` (its box), `Hull` (a UnionOperation's default: the convex hull of its corners, hollows filled in) or `PreciseConvexDecomposition` (convex pieces that follow its hollows, so a character walks through a doorway and a ball rolls into a crater). Precise takes longer to build, which is slow for a big model. A union has no pieces, so Default is its hull.",
    ),
    (
        ClassName::MeshPart,
        "WindAffected",
        "If true and the part isn't anchored, workspace.GlobalWind pushes it, harder for parts with bigger faces.",
    ),
    (
        ClassName::MeshPart,
        "CollisionGroup",
        "The collision group it's in, `\"Default\"` at first. Parts in two groups set not to collide (the Collision Groups window, or `PhysicsService:CollisionGroupSetCollidable`) pass through each other and don't touch. A name that isn't registered counts as Default.",
    ),
    (
        ClassName::MeshPart,
        "CanQuery",
        "If false, scripts' raycasts, shape casts and overlap queries (`workspace:Raycast`, `GetPartsInPart`, ...) pass it by. It still collides.",
    ),
    (
        ClassName::MeshPart,
        "CanTouch",
        "If false, it fires no Touched or TouchEnded, and touching it fires neither on other parts. It still collides.",
    ),
    (
        ClassName::MeshPart,
        "LinearDrag",
        "Air resistance on its speed: each second its assembly loses about that share of it (1 takes away about 63%). 0 at first. Welded parts use their root part's.",
    ),
    (
        ClassName::MeshPart,
        "AngularDrag",
        "Air resistance on its spin, like LinearDrag. 0.5 at first, so a spinning part slows by itself; 0 spins on.",
    ),
    (
        ClassName::MeshPart,
        "RollingFriction",
        "Extra spin slowing while its assembly touches anything, so a ball rolls to a stop on the ground but spins on in the air. 0 at first. Welded parts use their root part's.",
    ),
    (
        ClassName::MeshPart,
        "Massless",
        "If true, the part adds no mass to an assembly it shares with other parts. A part on its own (every part, until welds exist) keeps its own mass.",
    ),
    (
        ClassName::MeshPart,
        "CustomPhysicalProperties",
        "Its own density, friction, elasticity and their weights (`PhysicalProperties.new(...)`), or nil for Plastic's. Read `CurrentPhysicalProperties` for the ones in use.",
    ),
    (
        ClassName::MeshPart,
        "AssemblyLinearVelocity",
        "How fast the part moves, in studs a second. Physics updates it as the part moves; setting it throws the part. Anchored parts don't move.",
    ),
    (
        ClassName::MeshPart,
        "AssemblyAngularVelocity",
        "How fast the part spins, in radians a second around each world axis. Physics updates it; setting it spins the part.",
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
        ClassName::Terrain,
        "Water",
        "Floods the terrain's hollows up to WaterLevel with water that has waves, foam and reflections.",
    ),
    (
        ClassName::Terrain,
        "WaterLevel",
        "The world Y of the water's surface at rest. Ground below it is under water.",
    ),
    (
        ClassName::Terrain,
        "WaterColor",
        "The color deep water shows.",
    ),
    (
        ClassName::Terrain,
        "WaterClarity",
        "How many studs of water light crosses before only about a third of what's under it still shows. Bigger is clearer.",
    ),
    (
        ClassName::Terrain,
        "WaveHeight",
        "How high the biggest wave rises above the water's surface at rest, in studs.",
    ),
    (
        ClassName::Terrain,
        "WaveLength",
        "How long the biggest wave is from crest to crest, in studs.",
    ),
    (
        ClassName::Terrain,
        "WaveSpeed",
        "How fast the waves roll, as a share of how fast real waves of their length would. 0 stills them.",
    ),
    (
        ClassName::Terrain,
        "WaveDirection",
        "Which way the waves roll, in degrees: 0 toward +X, 90 toward +Z.",
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
        "FootPlacement",
        "If true, a skinned character plants each foot on what's under it on slopes and stairs, and lowers its hips to match, on each device. It needs an Animator and a skeleton with `LeftFoot` and `RightFoot` bones.",
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
        "FootPlacement",
        "If true, a skinned character plants each foot on what's under it on slopes and stairs, and lowers its hips to match, on each device. It needs an Animator and a skeleton with `LeftFoot` and `RightFoot` bones.",
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
        ClassName::ThirdPersonController,
        "Sit",
        "True while the character sits on a Seat (its Movement reads \"Seated\"). Set it false to stand the character up a little in front of the seat, as Jump does. Setting it true does nothing; use `seat:Sit(controller)`.",
    ),
    (
        ClassName::FirstPersonController,
        "Sit",
        "True while the character sits on a Seat (its Movement reads \"Seated\"). Set it false to stand the character up a little in front of the seat, as Jump does. Setting it true does nothing; use `seat:Sit(controller)`.",
    ),
    (
        ClassName::ThirdPersonController,
        "MoveDirection",
        "Where the character walks while no move key is held for it, across the ground (Y is ignored); a length under 1 walks that much slower. A controller that doesn't receive input (an NPC's) walks it too. Humanoid:Move and MoveTo set it.",
    ),
    (
        ClassName::ThirdPersonController,
        "PlatformStand",
        "If true, the controller neither moves nor jumps the character; it stays where it is until this is false again.",
    ),
    (
        ClassName::ThirdPersonController,
        "AutoRotate",
        "If true, the character turns about the middle of its body to face the way it walks (a first-person one, the way the camera looks). It's taken to face -Z when Play starts.",
    ),
    (
        ClassName::FirstPersonController,
        "MoveDirection",
        "Where the character walks while no move key is held for it, across the ground (Y is ignored); a length under 1 walks that much slower. A controller that doesn't receive input (an NPC's) walks it too. Humanoid:Move and MoveTo set it.",
    ),
    (
        ClassName::FirstPersonController,
        "PlatformStand",
        "If true, the controller neither moves nor jumps the character; it stays where it is until this is false again.",
    ),
    (
        ClassName::FirstPersonController,
        "AutoRotate",
        "If true, the character turns about the middle of its body to face the way it walks (a first-person one, the way the camera looks). It's taken to face -Z when Play starts.",
    ),
    (
        ClassName::Seat,
        "Occupant",
        "The character controller sitting on the seat, or nil. Read-only: `seat:Sit(controller)` sits one, and `controller.Sit = false` stands it up.",
    ),
    (
        ClassName::Seat,
        "Disabled",
        "If true, characters that touch the seat don't sit on it. `seat:Sit(controller)` still works.",
    ),
    (
        ClassName::Seat,
        "SeatOffset",
        "An Attachment where a sitting character's feet go, or nil for the middle of the seat's top face.",
    ),
    (
        ClassName::VehicleSeat,
        "MaxSpeed",
        "Studs a second the driven wheels stop pushing at. They push less the closer the car gets.",
    ),
    (
        ClassName::VehicleSeat,
        "Torque",
        "How hard the driven wheels push at full Throttle: studs a second squared on flat ground, whatever the car weighs. At the default 80 a car climbs slopes up to about 24°.",
    ),
    (
        ClassName::VehicleSeat,
        "TurnSpeed",
        "Degrees a second the steering wheels turn toward where Steer points them.",
    ),
    (
        ClassName::VehicleSeat,
        "SteerAngle",
        "Degrees the wheels with Steers on turn at full Steer.",
    ),
    (
        ClassName::VehicleSeat,
        "Throttle",
        "-1 (full reverse) to 1 (full forward). While someone sits on the seat, the Move action sets it every frame (W and S, or a stick); scripts set it to drive an empty car. Getting out sets it to 0.",
    ),
    (
        ClassName::VehicleSeat,
        "Steer",
        "-1 (left) to 1 (right), set like Throttle (A and D, or a stick).",
    ),
    (
        ClassName::VehicleSeat,
        "HandBrake",
        "If true, the wheels with Brakes on hold the car still. An empty seat with no Throttle holds the car the same way; otherwise a car with no Throttle coasts, slowing a little.",
    ),
    (
        ClassName::VehicleWheel,
        "Position",
        "The wheel's middle with its spring at rest length, in studs from the middle of its part, along the part's own axes.",
    ),
    (
        ClassName::VehicleWheel,
        "Orientation",
        "How the wheel is turned relative to its part, in degrees like a Part's Orientation. The wheel rolls toward its -Z, turns on its X axle, and its spring is along its Y.",
    ),
    (
        ClassName::VehicleWheel,
        "Radius",
        "The wheel's radius in studs. Its bottom is this far below its middle.",
    ),
    (
        ClassName::VehicleWheel,
        "SuspensionRestLength",
        "Studs of spring above the wheel's middle. The car's weight squashes it; a wheel off the ground hangs at this length.",
    ),
    (
        ClassName::VehicleWheel,
        "SuspensionStiffness",
        "How hard the spring pushes back for each stud it's squashed, whatever the car weighs. With four wheels, the default 100 squashes each half a stud at rest.",
    ),
    (
        ClassName::VehicleWheel,
        "SuspensionDamping",
        "How much the spring holds back bouncing. Higher settles the car sooner; the engine lowers it where more would make the car rock.",
    ),
    (
        ClassName::VehicleWheel,
        "Friction",
        "How much grip the tyre has before it slides, braking or pushing.",
    ),
    (
        ClassName::VehicleWheel,
        "Steers",
        "If true, the wheel turns with the seat's Steer.",
    ),
    (
        ClassName::VehicleWheel,
        "Driven",
        "If true, the wheel pushes with the seat's Throttle.",
    ),
    (
        ClassName::VehicleWheel,
        "Brakes",
        "If true, the wheel holds the car on the HandBrake, and when nobody drives.",
    ),
    (
        ClassName::VehicleWheel,
        "WheelPart",
        "A part moved to the wheel every frame during Play, spinning as it rolls and turning as it steers: its middle at the wheel's and its X along the axle, as a Cylinder's round faces are. Make it Anchored with CanCollide off; nil shows nothing.",
    ),
    (
        ClassName::Ragdoll,
        "Enabled",
        "If true, physics moves the ragdoll's capsules and they pose the bones (the MeshPart follows the hips across the ground). Setting it false blends back to the Animator's pose over BlendTime, after playing GetUpState.",
    ),
    (
        ClassName::Ragdoll,
        "BlendTime",
        "Seconds to blend from where the ragdoll lies back to the Animator's pose once Enabled is false.",
    ),
    (
        ClassName::Ragdoll,
        "GetUpState",
        "The name of the Animator's state to play (with no blend of its own) when Enabled turns false, such as a getting-up clip, or empty to carry on with the state playing.",
    ),
    (
        ClassName::IKControl,
        "Type",
        "How it turns the bones: `\"Position\"` puts EndEffector at the Target, `\"Rotation\"` turns it as the Target is turned from the MeshPart, `\"Transform\"` does both, and `\"LookAt\"` faces it at the Target.",
    ),
    (
        ClassName::IKControl,
        "Enabled",
        "If false, the pose goes back to the plain animation over SmoothTime.",
    ),
    (
        ClassName::IKControl,
        "EndEffector",
        "The bone that reaches or faces the Target, by its name or path, such as `\"Head\"` (which finds `mixamorig:Head`).",
    ),
    (
        ClassName::IKControl,
        "ChainRoot",
        "The highest bone that turns. Empty is the two bones above EndEffector for Position and Transform, or just EndEffector for LookAt and Rotation.",
    ),
    (
        ClassName::IKControl,
        "Target",
        "The Part or Attachment to reach or face. With none, the pose goes back to the plain animation over SmoothTime.",
    ),
    (
        ClassName::IKControl,
        "Pole",
        "A Part or Attachment that the elbow or knee of a Position or Transform chain bends toward. With none, the limb bends the way it already bends.",
    ),
    (
        ClassName::IKControl,
        "Weight",
        "How much of the solved pose is used, from 0 (the plain animation) to 1. A change fades in over SmoothTime.",
    ),
    (
        ClassName::IKControl,
        "SmoothTime",
        "Seconds a change in Weight or Enabled takes, and about how long the IK trails its Target. 0 follows at once.",
    ),
    (
        ClassName::IKControl,
        "MaxAngle",
        "LookAt only: the most degrees the EndEffector turns from the way the character faces.",
    ),
    (
        ClassName::IKControl,
        "Forward",
        "LookAt only: the way the EndEffector faces in the MeshPart's own axes, such as `Vector3.new(0, 0, 1)`. (0, 0, 0) works it out from the way the skeleton's toes point, or else -Z.",
    ),
    (
        ClassName::StarterPlayer,
        "CharacterFootPlacement",
        "If true, each player's character spawns with its controller's FootPlacement on. If false, a StarterCharacter prefab keeps its own setting.",
    ),
    (ClassName::WeldConstraint, "BreakForce", BREAK_FORCE_DOC),
    (ClassName::WeldConstraint, "BreakTorque", BREAK_TORQUE_DOC),
    (ClassName::HingeConstraint, "BreakForce", BREAK_FORCE_DOC),
    (ClassName::HingeConstraint, "BreakTorque", BREAK_TORQUE_DOC),
    (
        ClassName::BallSocketConstraint,
        "BreakForce",
        BREAK_FORCE_DOC,
    ),
    (
        ClassName::BallSocketConstraint,
        "BreakTorque",
        BREAK_TORQUE_DOC,
    ),
    (
        ClassName::PrismaticConstraint,
        "BreakForce",
        BREAK_FORCE_DOC,
    ),
    (
        ClassName::PrismaticConstraint,
        "BreakTorque",
        BREAK_TORQUE_DOC,
    ),
    (
        ClassName::CylindricalConstraint,
        "BreakForce",
        BREAK_FORCE_DOC,
    ),
    (
        ClassName::CylindricalConstraint,
        "BreakTorque",
        BREAK_TORQUE_DOC,
    ),
    (ClassName::SpringConstraint, "BreakForce", BREAK_FORCE_DOC),
    (ClassName::SpringConstraint, "BreakTorque", BREAK_TORQUE_DOC),
    (ClassName::RopeConstraint, "BreakForce", BREAK_FORCE_DOC),
    (ClassName::RopeConstraint, "BreakTorque", BREAK_TORQUE_DOC),
    (ClassName::RodConstraint, "BreakForce", BREAK_FORCE_DOC),
    (ClassName::RodConstraint, "BreakTorque", BREAK_TORQUE_DOC),
    (
        ClassName::Fracture,
        "Pieces",
        "How many pieces Generate (in its Properties) cuts the part into.",
    ),
    (
        ClassName::Fracture,
        "Seed",
        "The same Seed always makes the same pieces; change it for a different break.",
    ),
    (
        ClassName::Fracture,
        "Pattern",
        "`Enum.FracturePattern.Uniform` makes pieces of about one size; `Impact` makes more, smaller pieces toward the part's middle, as where something struck it.",
    ),
    (
        ClassName::Fracture,
        "InteriorMaterial",
        "What the faces the cuts make inside the part look like, in the part's color.",
    ),
    (
        ClassName::Fracture,
        "Threshold",
        "How fast, in studs a second, something must hit the part (the two closing at the point they touch) to shatter it. 0 or less: only `Shatter` breaks it.",
    ),
    (
        ClassName::Fracture,
        "DebrisLifetime",
        "Seconds the pieces last once the part shatters. They fade out over the last second and are destroyed. 0 keeps them.",
    ),
    (
        ClassName::Fracture,
        "PiecesId",
        "The pieces' file, which Generate writes: `<name>.fracture.glb` next to the MeshPart's model, or in `assets/models` for a Part. Empty until it's generated; the part can't shatter until then.",
    ),
    (
        ClassName::Health,
        "MaxHealth",
        "The most Health it can have. Heal and regeneration stop here, and lowering it lowers a Health above it.",
    ),
    (
        ClassName::Health,
        "Health",
        "How much is left, 0 to MaxHealth. Setting it fires HealthChanged (and Died, if it reaches 0). At 0 the Model is dead: damage and Heal do nothing, and it doesn't regenerate, until a script sets Health again.",
    ),
    (
        ClassName::Health,
        "RegenRate",
        "Health a second won back once RegenDelay seconds pass without damage, up to MaxHealth. 0 never regenerates.",
    ),
    (
        ClassName::Health,
        "RegenDelay",
        "Seconds without damage before it starts regenerating. Any damage, from TakeDamage, a DamageZone or a fall, starts the wait again.",
    ),
    (
        ClassName::Health,
        "Invulnerable",
        "If true, TakeDamage, DamageZones and falls do nothing to it. Heal still heals.",
    ),
    (
        ClassName::Health,
        "FallDamage",
        "If true, the character controller in its Model loses health when it lands faster than FallDamageThreshold: FallDamageScale for each stud a second over it. Off by default.",
    ),
    (
        ClassName::Health,
        "FallDamageThreshold",
        "Studs a second a character can land at without damage. At the default Gravity, 60 is a fall of about 9 studs; a jump lands at about 53.",
    ),
    (
        ClassName::Health,
        "FallDamageScale",
        "Health lost for each stud a second a landing is over FallDamageThreshold.",
    ),
    (
        ClassName::Health,
        "DisplayName",
        "The name a game shows over the character. Empty is the Model's Name. A Humanoid's DisplayName is this.",
    ),
    (
        ClassName::Health,
        "HealthDisplayDistance",
        "How far away, in studs, a game shows the character's health over it; 0 never. A Humanoid's HealthDisplayDistance is this.",
    ),
    (
        ClassName::ProximityPrompt,
        "ActionText",
        "What it does, such as \"Open\", shown in the prompt before ObjectText.",
    ),
    (
        ClassName::ProximityPrompt,
        "ObjectText",
        "What it does it to, such as \"Door\": the prompt shows \"Open · Door\". Empty shows ActionText alone.",
    ),
    (
        ClassName::ProximityPrompt,
        "Action",
        "The input map action that uses it: Interact (E, or X on a controller) by default. The prompt shows the key, or the controller button once a controller was used last, with the player's rebinds.",
    ),
    (
        ClassName::ProximityPrompt,
        "HoldDuration",
        "Seconds the action is held before Triggered fires; the prompt's ring fills meanwhile. 0 triggers on the press.",
    ),
    (
        ClassName::ProximityPrompt,
        "MaxActivationDistance",
        "Studs from the middle of the player character (or the camera, with no character) it shows within.",
    ),
    (
        ClassName::ProximityPrompt,
        "RequiresLineOfSight",
        "If true, it doesn't show while a part that can be hit by queries is between it and the character. Its own part and the character don't count.",
    ),
    (
        ClassName::ProximityPrompt,
        "Enabled",
        "If false, it never shows, and one showing hides.",
    ),
    (
        ClassName::ProximityPrompt,
        "Exclusivity",
        "`Enum.ProximityPromptExclusivity`: OnePerButton shows the nearest prompt for each action, OneGlobally the nearest of all, and AlwaysShow shows this one whenever it's in reach.",
    ),
    (
        ClassName::ProximityPrompt,
        "Dialogue",
        "A dialogue file (`assets/dialogue/Farmer.dialogue.json`) it starts when triggered, with no script, said by the Model it's in. Empty starts none.",
    ),
    (
        ClassName::ClickDetector,
        "MaxActivationDistance",
        "Studs from the middle of the player character (or the camera, with no character) to the point clicked. Farther clicks, and pointing from farther, do nothing.",
    ),
    (
        ClassName::ClickDetector,
        "CursorIcon",
        "The cursor over the part, as a CSS cursor name such as \"pointer\", \"crosshair\" or \"grab\". Empty is the pointing hand.",
    ),
    (
        ClassName::Tool,
        "Handle",
        "The part held in the hand and touched to pick the Tool up. nil uses its child part named \"Handle\".",
    ),
    (
        ClassName::Tool,
        "Grip",
        "The Attachment in the Handle that's put in the right hand, turned as the hand is. nil uses the Handle's Attachment named \"Grip\", else the Handle's middle.",
    ),
    (
        ClassName::Tool,
        "RequiresHandle",
        "If false, a Tool with no Handle can be carried and equipped (by scripts and the hotbar); it's never picked up by touch.",
    ),
    (
        ClassName::Tool,
        "LagCompensated",
        "If true, the server's `workspace:Raycast` calls in handlers of a RemoteEvent inside this Tool are lag compensated for the Player who fired it, until the handler first waits (see RaycastParams.CompensateFor).",
    ),
    (
        ClassName::Tool,
        "CanBeDropped",
        "If false, the Drop action and tool:Drop() leave it carried.",
    ),
    (
        ClassName::Tool,
        "ToolTip",
        "Text the hotbar shows over the bar while it's in hand.",
    ),
    (
        ClassName::Tool,
        "TextureId",
        "The image in its hotbar slot, such as \"assets/textures/sword.png\". Empty shows its Name.",
    ),
    (
        ClassName::Tool,
        "Item",
        "The data-table row it is, as \"table:key\" (such as \"weapons:sword\"), for scripts to read with DataService. Save slots keep it. Empty for none.",
    ),
    (
        ClassName::Inventory,
        "Capacity",
        "How many Tools it holds. A full Inventory leaves a touched Tool on the ground, and Add returns false.",
    ),
    (
        ClassName::Inventory,
        "Equipped",
        "The Tool in hand, or nil. Read-only: change it with Equip and UnequipAll.",
    ),
    (
        ClassName::Inventory,
        "HotbarEnabled",
        "If false, the default hotbar doesn't show and its number keys, wheel and bumpers do nothing, for a game that makes its own. Drop and Primary still work.",
    ),
    (
        ClassName::Humanoid,
        "BreakJointsOnDeath",
        "If true, dying turns off the WeldConstraints and joints in the character (not its Ragdolls') and unanchors its parts, so they fall apart.",
    ),
    (
        ClassName::Humanoid,
        "RagdollOnDeath",
        "If true, dying turns on every Ragdoll in the character.",
    ),
    (
        ClassName::Players,
        "CharacterAutoLoads",
        "If true, each player gets a character as they join (unless one is already in the Workspace, which becomes theirs) and a new one RespawnTime seconds after their character's Humanoid dies. If false, characters come only from `LoadCharacter`.",
    ),
    (
        ClassName::Players,
        "RespawnTime",
        "Seconds between a character's Humanoid dying and its player getting a new character, while CharacterAutoLoads is on.",
    ),
    (
        ClassName::Players,
        "LeaderboardEnabled",
        "If false, the default leaderboard isn't drawn, for a game that makes its own. It shows each Player's `leaderstats` Folder of Value objects, top right, and Tab shows and hides it.",
    ),
    (
        ClassName::Script,
        "SourceFile",
        "For a script inside an object: the project file its code is in, such as `objects/Door/Open.server.lua`, which every copy shares. Empty for a script in the `scripts` folder. Changing it while the game runs doesn't change the code that runs.",
    ),
    (
        ClassName::LocalScript,
        "SourceFile",
        "For a script inside an object: the project file its code is in, such as `objects/Door/Open.client.lua`, which every copy shares. Empty for a script in the `scripts` folder. Changing it while the game runs doesn't change the code that runs.",
    ),
    (
        ClassName::ModuleScript,
        "SourceFile",
        "For a script inside an object: the project file its code is in, such as `objects/Door/Open.module.lua`, which every copy shares. Empty for a script in the `scripts` folder. Changing it while the game runs doesn't change the code that runs.",
    ),
    (
        ClassName::DataLayer,
        "Color",
        "The colour the editor's Layers panel marks the layer with (roadmap R16).",
    ),
    (
        ClassName::DataLayer,
        "ActiveOnStart",
        "If false, a game starts with the layer's Instances set aside, until a script makes it active (roadmap R16.4). The editor shows every layer whatever this is; hiding one there is the Layers panel's eye.",
    ),
    (
        ClassName::DataLayer,
        "Active",
        "Whether the layer is on in the running game (roadmap R16.4): a game starts it at ActiveOnStart. Read-only: DataLayerService:SetLayerActive switches it, and every device sees the change.",
    ),
    (
        ClassName::TextChatService,
        "ChatEnabled",
        "If false, the default chat window isn't drawn and `/` doesn't open it, for a game with its own chat or none. Scripts can still send and hear messages.",
    ),
    (
        ClassName::Players,
        "MaxPlayers",
        "How many players a hosted game takes, the host's own player among them. A player who joins a full game is refused, and their `NetworkService.ConnectionFailed` says the game is full.",
    ),
    (
        ClassName::StarterPlayer,
        "StarterCharacter",
        "The prefab (`assets/prefabs/*.prefab.json`) each player's character is a copy of. Empty uses the character Insert > Character makes: six welded parts with a Humanoid, a ThirdPersonController, a Health, an Inventory and an Animator.",
    ),
    (
        ClassName::Player,
        "Character",
        "The character Model the player plays as, or nil. Set by spawning and `LoadCharacter`; a character placed in the Workspace before Play becomes the player's.",
    ),
    (
        ClassName::Player,
        "Team",
        "The Team the player is on, or nil. Setting it also sets Neutral (false on a team, true on nil). Players join the AutoAssignable Team with the fewest players.",
    ),
    (
        ClassName::Player,
        "Neutral",
        "If true, the player is on no team, and spawns only on Neutral SpawnLocations.",
    ),
    (
        ClassName::Player,
        "UserId",
        "Who plays: a number that stays the same each time they play on one install of Axcenity, so a server can recognize them when they join again. Read-only.",
    ),
    (
        ClassName::Player,
        "DisplayName",
        "The name the player joined with. `Name` is the same, but numbered when someone in the game already has it (`Player2`). Read-only.",
    ),
    (
        ClassName::Team,
        "TeamColor",
        "The team's color: its players spawn on SpawnLocations of this TeamColor.",
    ),
    (
        ClassName::Team,
        "AutoAssignable",
        "If true, players joining are put on this team, or on the AutoAssignable team with the fewest players when there are several.",
    ),
    (
        ClassName::SpawnLocation,
        "Enabled",
        "If false, nobody spawns here.",
    ),
    (
        ClassName::SpawnLocation,
        "Neutral",
        "If true, any player can spawn here: neutral players, and players whose team has no enabled SpawnLocation of its own. If false, only players on the Team of its TeamColor.",
    ),
    (
        ClassName::SpawnLocation,
        "TeamColor",
        "While Neutral is off, the players of the Team with this TeamColor spawn here.",
    ),
    (
        ClassName::SpawnLocation,
        "Duration",
        "Seconds a ForceField keeps a character spawned here from losing Health. 0 gives none.",
    ),
    (
        ClassName::DamageZone,
        "DamagePerSecond",
        "Health a second taken from every Health whose Model has a part inside the zone (once each, however many of its parts are in). The damage's source is the zone. 0 or less does nothing.",
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
        ClassName::NavigationAgent,
        "PatrolSpline",
        "A Spline whose points the agent walks to in order during Play, with no script: round again from the last to the first on a Closed spline, back and forth on an open one. It walks the navigation mesh between points, so it goes round walls; a point it can't reach is skipped. nil patrols nothing. `agent:Patrol(spline)` sets it.",
    ),
    (
        ClassName::Spline,
        "Closed",
        "If true, the curve goes on from the last SplinePoint back to the first, as a loop.",
    ),
    (
        ClassName::SplinePoint,
        "Position",
        "Where the curve passes, in the world. Its place among the Spline's children is its place along the curve.",
    ),
    (
        ClassName::SplinePoint,
        "TangentMode",
        "How the curve bends here: `Enum.SplineTangentMode.Auto` (the default) passes through smoothly, from where its neighbours are; `Linear` heads straight for its neighbours (two Linear points are joined by a straight line); `Free` uses InTangent and OutTangent.",
    ),
    (
        ClassName::SplinePoint,
        "InTangent",
        "With TangentMode Free: from the point to the handle the curve arrives along, in studs.",
    ),
    (
        ClassName::SplinePoint,
        "OutTangent",
        "With TangentMode Free: from the point to the handle the curve leaves along, in studs. Longer handles bend the curve further their way.",
    ),
    (
        ClassName::Ability,
        "Cooldown",
        "Seconds after it activates before it can activate again.",
    ),
    (
        ClassName::Ability,
        "CostAttribute",
        "The number attribute activating spends, such as `\"Mana\"`, of its holder: the Model or Player it's in, or that Player's character or that character's Player. Empty costs nothing.",
    ),
    (
        ClassName::Ability,
        "CostAmount",
        "How much of CostAttribute activating spends. It can't activate while its holder has less.",
    ),
    (
        ClassName::Ability,
        "ActivationKey",
        "The input map action, such as `\"Ability1\"`, that activates it for the player whose character or Player holds it. The ability bar shows it. Empty: only scripts activate it.",
    ),
    (
        ClassName::Ability,
        "BlockedByTags",
        "Tags, separated by commas, such as `\"Stunned, Silenced\"`: while its holder has any of them, it can't activate.",
    ),
    (
        ClassName::Ability,
        "GrantsTags",
        "Tags, separated by commas, its holder's character has while it's active (for its Duration).",
    ),
    (
        ClassName::Ability,
        "Duration",
        "Seconds it stays active after activating, giving its GrantsTags, before Ended fires. 0 ends at once.",
    ),
    (
        ClassName::Ability,
        "Icon",
        "The picture the ability bar shows for it, such as `\"assets/textures/fireball.png\"`. Empty shows its Name.",
    ),
    (
        ClassName::Ability,
        "CooldownLeft",
        "Read-only: seconds until it can activate again. 0 when it's ready.",
    ),
    (
        ClassName::Ability,
        "Active",
        "Read-only: true from activating until Ended, for an Ability with a Duration.",
    ),
    (
        ClassName::Effect,
        "Attribute",
        "What it changes on the Model it's in: a property of its Health (`Health`, `MaxHealth`, `RegenRate`), of its character controller, as the Humanoid's (`WalkSpeed`, `JumpHeight`), or else the Model's number attribute of this name.",
    ),
    (
        ClassName::Effect,
        "Operation",
        "`Enum.EffectOperation.Add` adds Amount (a negative Amount on Health is damage); `Multiply` multiplies by it. Each stack does it again.",
    ),
    (
        ClassName::Effect,
        "Amount",
        "How much it adds, or what it multiplies by.",
    ),
    (
        ClassName::Effect,
        "Duration",
        "Seconds it lasts. 0 is instant: it changes once and is gone. Below 0 it lasts until it's removed.",
    ),
    (
        ClassName::Effect,
        "Period",
        "With a Duration: above 0, it changes again every Period seconds, for good (poison, a heal over time). 0 changes once for as long as it lasts, and removing it undoes that.",
    ),
    (
        ClassName::Effect,
        "MaxStacks",
        "How many times applying it to a Model that has it already adds to it. Applying it again also starts its Duration over.",
    ),
    (
        ClassName::Effect,
        "GrantsTags",
        "Tags, separated by commas, its Model has while it's active.",
    ),
    (
        ClassName::Effect,
        "Stacks",
        "Read-only: how many times it's applied.",
    ),
    (
        ClassName::Effect,
        "TimeLeft",
        "Read-only: the seconds it has left.",
    ),
    (
        ClassName::SplineFollower,
        "Spline",
        "The Spline it moves its parent along. nil moves nothing.",
    ),
    (
        ClassName::SplineFollower,
        "Speed",
        "Studs a second along the curve, the same speed however its points are spaced.",
    ),
    (
        ClassName::SplineFollower,
        "Mode",
        "What it does at the ends: `Enum.SplineFollowMode.Loop` goes on from the start again (round a Closed spline), `PingPong` (the default) turns back at each end, and `Once` stops at the end.",
    ),
    (
        ClassName::SplineFollower,
        "FaceAlong",
        "If true, its parent turns to face the way it goes along the curve, upright, as a train does. If false (the default), it keeps the way it was turned.",
    ),
    (
        ClassName::SplineFollower,
        "Enabled",
        "If false, it stops where it is; true carries on from there.",
    ),
    (
        ClassName::SplineMesh,
        "MeshId",
        "The model bent along the Spline it's in, such as \"assets/models/road.glb\": a straight piece whose length runs along its Z. Empty makes nothing.",
    ),
    (
        ClassName::SplineMesh,
        "MaterialVariant",
        "A material file its pieces are drawn with in place of the model's materials. Empty for the model's own.",
    ),
    (
        ClassName::SplineMesh,
        "Size",
        "One piece in studs: its width (X), height (Y, up from the curve) and length (Z, along it). A whole number of pieces fills the curve, each stretched a little to fit. (8, 1, 8) by default.",
    ),
    (
        ClassName::SplineMesh,
        "CanCollide",
        "If true (the default), its pieces collide as their bent shapes, so cars drive and characters walk on a road.",
    ),
    (
        ClassName::SplineMesh,
        "CastShadow",
        "If true (the default), its pieces cast shadows.",
    ),
    (
        ClassName::SplineScatter,
        "Models",
        "The model files it places, separated by `;`, such as \"assets/models/tree.glb; assets/models/rock.glb\". Each spot picks one. In the editor, click models in the Assets panel while it's selected to add or take them out.",
    ),
    (
        ClassName::SplineScatter,
        "Seed",
        "Which random layout it makes: the same seed, spline and settings give the same layout every time. 1 by default.",
    ),
    (
        ClassName::SplineScatter,
        "Spacing",
        "Studs between models along the curve (at least 0.5). 8 by default.",
    ),
    (
        ClassName::SplineScatter,
        "FillInside",
        "If true, it fills the inside of a Closed spline at Density instead of placing models along the curve. False by default.",
    ),
    (
        ClassName::SplineScatter,
        "Density",
        "Models per 100 square studs inside a Closed spline, with FillInside. 1 by default.",
    ),
    (
        ClassName::SplineScatter,
        "RotationRange",
        "Each model is turned round its up by a random number of degrees between X and Y. (0, 360) by default.",
    ),
    (
        ClassName::SplineScatter,
        "ScaleRange",
        "Each model is its own size times a random number between X and Y. (1, 1) by default.",
    ),
    (
        ClassName::SplineScatter,
        "Offset",
        "Each model along the curve is moved up to this many studs to one side or the other at random. 0 by default.",
    ),
    (
        ClassName::SplineScatter,
        "AlignToSurface",
        "If true (the default), each model stands out from the surface below it, tilted with the ground; if false, upright.",
    ),
    (
        ClassName::SplineScatter,
        "MaxSlope",
        "Ground steeper than this many degrees gets no model. 35 by default.",
    ),
    (
        ClassName::SplineScatter,
        "CanCollide",
        "If true (the default), the models it places collide.",
    ),
    (
        ClassName::IntValue,
        "Value",
        "The whole number it holds; others are rounded. 0 by default.",
    ),
    (
        ClassName::NumberValue,
        "Value",
        "The number it holds. 0 by default.",
    ),
    (
        ClassName::StringValue,
        "Value",
        "The string it holds. Empty by default.",
    ),
    (
        ClassName::BoolValue,
        "Value",
        "The boolean it holds. False by default.",
    ),
    (
        ClassName::Vector3Value,
        "Value",
        "The Vector3 it holds. Zero by default.",
    ),
    (
        ClassName::Color3Value,
        "Value",
        "The Color3 it holds. Black by default.",
    ),
    (
        ClassName::CFrameValue,
        "Value",
        "The CFrame it holds. At the origin, unturned, by default.",
    ),
    (
        ClassName::ObjectValue,
        "Value",
        "The Instance it points at, or nil (the default).",
    ),
    (
        ClassName::Checkpoint,
        "Position",
        "Where SaveService:RestoreCheckpoint stands the player character: the middle of its feet goes here.",
    ),
    (
        ClassName::Attachment,
        "Position",
        "Where it is, in studs from the middle of its part, along the part's own axes.",
    ),
    (
        ClassName::Attachment,
        "Orientation",
        "How it's turned relative to its part, in degrees like a Part's Orientation.",
    ),
    (
        ClassName::WeldConstraint,
        "Part0",
        "One of the two parts it holds together.",
    ),
    (
        ClassName::WeldConstraint,
        "Part1",
        "The other part it holds together.",
    ),
    (
        ClassName::WeldConstraint,
        "Enabled",
        "On (the default), it holds the parts together. Off, they move apart; turned on again, it welds them where they are then.",
    ),
    (
        ClassName::HingeConstraint,
        "Attachment0",
        "The Attachment on the first part. The joint's axis is this attachment's X axis (its `Axis`).",
    ),
    (
        ClassName::HingeConstraint,
        "Attachment1",
        "The Attachment on the second part, held to Attachment0.",
    ),
    (
        ClassName::HingeConstraint,
        "Enabled",
        "On (the default), it joins the attachments' parts. Off, it holds nothing.",
    ),
    (
        ClassName::HingeConstraint,
        "Visible",
        "On, the game draws it in Color: a coil for a spring, a line for a rope or rod, an axle or ball for the others. Off by default; the editor's Constraints overlay always shows it.",
    ),
    (
        ClassName::HingeConstraint,
        "Color",
        "The color it's drawn in while Visible.",
    ),
    (
        ClassName::HingeConstraint,
        "LimitsEnabled",
        "On, Attachment1 only turns between LowerAngle and UpperAngle.",
    ),
    (
        ClassName::HingeConstraint,
        "LowerAngle",
        "While LimitsEnabled is on, the farthest it turns one way, in degrees.",
    ),
    (
        ClassName::HingeConstraint,
        "UpperAngle",
        "While LimitsEnabled is on, the farthest it turns the other way, in degrees.",
    ),
    (
        ClassName::HingeConstraint,
        "ActuatorType",
        "What turns it: `Enum.ActuatorType.None` (free), `Motor` (spins at AngularVelocity) or `Servo` (turns to TargetAngle and holds it).",
    ),
    (
        ClassName::HingeConstraint,
        "AngularVelocity",
        "With a Motor actuator, how fast it turns Attachment1 around the axis, in radians a second.",
    ),
    (
        ClassName::HingeConstraint,
        "MotorMaxTorque",
        "With a Motor actuator, the most torque it turns with. `math.huge` has no limit.",
    ),
    (
        ClassName::HingeConstraint,
        "TargetAngle",
        "With a Servo actuator, the angle it turns Attachment1 to and holds, in degrees.",
    ),
    (
        ClassName::HingeConstraint,
        "AngularSpeed",
        "With a Servo actuator, the fastest it turns toward TargetAngle, in radians a second.",
    ),
    (
        ClassName::HingeConstraint,
        "ServoMaxTorque",
        "With a Servo actuator, the most torque it turns and holds with. Too little, and a heavy load drags it.",
    ),
    (
        ClassName::BallSocketConstraint,
        "Attachment0",
        "The Attachment on the first part. The joint's axis is this attachment's X axis (its `Axis`).",
    ),
    (
        ClassName::BallSocketConstraint,
        "Attachment1",
        "The Attachment on the second part, held to Attachment0.",
    ),
    (
        ClassName::BallSocketConstraint,
        "Enabled",
        "On (the default), it joins the attachments' parts. Off, it holds nothing.",
    ),
    (
        ClassName::BallSocketConstraint,
        "Visible",
        "On, the game draws it in Color: a coil for a spring, a line for a rope or rod, an axle or ball for the others. Off by default; the editor's Constraints overlay always shows it.",
    ),
    (
        ClassName::BallSocketConstraint,
        "Color",
        "The color it's drawn in while Visible.",
    ),
    (
        ClassName::BallSocketConstraint,
        "LimitsEnabled",
        "On, Attachment1's axis can only lean UpperAngle from Attachment0's.",
    ),
    (
        ClassName::BallSocketConstraint,
        "UpperAngle",
        "While LimitsEnabled is on, how far Attachment1's axis can lean from Attachment0's, in degrees.",
    ),
    (
        ClassName::BallSocketConstraint,
        "TwistLimitsEnabled",
        "On, Attachment1 can only twist around its axis between TwistLowerAngle and TwistUpperAngle.",
    ),
    (
        ClassName::BallSocketConstraint,
        "TwistLowerAngle",
        "While TwistLimitsEnabled is on, the farthest it twists one way, in degrees.",
    ),
    (
        ClassName::BallSocketConstraint,
        "TwistUpperAngle",
        "While TwistLimitsEnabled is on, the farthest it twists the other way, in degrees.",
    ),
    (
        ClassName::PrismaticConstraint,
        "Attachment0",
        "The Attachment on the first part. The joint's axis is this attachment's X axis (its `Axis`).",
    ),
    (
        ClassName::PrismaticConstraint,
        "Attachment1",
        "The Attachment on the second part, held to Attachment0.",
    ),
    (
        ClassName::PrismaticConstraint,
        "Enabled",
        "On (the default), it joins the attachments' parts. Off, it holds nothing.",
    ),
    (
        ClassName::PrismaticConstraint,
        "Visible",
        "On, the game draws it in Color: a coil for a spring, a line for a rope or rod, an axle or ball for the others. Off by default; the editor's Constraints overlay always shows it.",
    ),
    (
        ClassName::PrismaticConstraint,
        "Color",
        "The color it's drawn in while Visible.",
    ),
    (
        ClassName::PrismaticConstraint,
        "LimitsEnabled",
        "On, Attachment1 only slides between LowerLimit and UpperLimit along the axis.",
    ),
    (
        ClassName::PrismaticConstraint,
        "LowerLimit",
        "While LimitsEnabled is on, the closest Attachment1 can be to Attachment0 along the axis, in studs (negative is behind it).",
    ),
    (
        ClassName::PrismaticConstraint,
        "UpperLimit",
        "While LimitsEnabled is on, the farthest Attachment1 can slide along the axis, in studs.",
    ),
    (
        ClassName::PrismaticConstraint,
        "ActuatorType",
        "What slides it: `Enum.ActuatorType.None` (free), `Motor` (slides at Velocity) or `Servo` (slides to TargetPosition and holds it).",
    ),
    (
        ClassName::PrismaticConstraint,
        "Velocity",
        "With a Motor ActuatorType, how fast it slides Attachment1 along the axis, in studs a second.",
    ),
    (
        ClassName::PrismaticConstraint,
        "MotorMaxForce",
        "With a Motor ActuatorType, the most force it slides with. `math.huge` has no limit.",
    ),
    (
        ClassName::PrismaticConstraint,
        "TargetPosition",
        "With a Servo ActuatorType, how far along the axis it slides Attachment1 to and holds, in studs.",
    ),
    (
        ClassName::PrismaticConstraint,
        "Speed",
        "With a Servo ActuatorType, the fastest it slides toward TargetPosition, in studs a second.",
    ),
    (
        ClassName::PrismaticConstraint,
        "ServoMaxForce",
        "With a Servo ActuatorType, the most force it slides and holds with.",
    ),
    (
        ClassName::CylindricalConstraint,
        "Attachment0",
        "The Attachment on the first part. The joint's axis is this attachment's X axis (its `Axis`).",
    ),
    (
        ClassName::CylindricalConstraint,
        "Attachment1",
        "The Attachment on the second part, held to Attachment0.",
    ),
    (
        ClassName::CylindricalConstraint,
        "Enabled",
        "On (the default), it joins the attachments' parts. Off, it holds nothing.",
    ),
    (
        ClassName::CylindricalConstraint,
        "Visible",
        "On, the game draws it in Color: a coil for a spring, a line for a rope or rod, an axle or ball for the others. Off by default; the editor's Constraints overlay always shows it.",
    ),
    (
        ClassName::CylindricalConstraint,
        "Color",
        "The color it's drawn in while Visible.",
    ),
    (
        ClassName::CylindricalConstraint,
        "LimitsEnabled",
        "On, Attachment1 only slides between LowerLimit and UpperLimit along the axis.",
    ),
    (
        ClassName::CylindricalConstraint,
        "LowerLimit",
        "While LimitsEnabled is on, the closest Attachment1 can be to Attachment0 along the axis, in studs (negative is behind it).",
    ),
    (
        ClassName::CylindricalConstraint,
        "UpperLimit",
        "While LimitsEnabled is on, the farthest Attachment1 can slide along the axis, in studs.",
    ),
    (
        ClassName::CylindricalConstraint,
        "ActuatorType",
        "What slides it: `Enum.ActuatorType.None` (free), `Motor` (slides at Velocity) or `Servo` (slides to TargetPosition and holds it).",
    ),
    (
        ClassName::CylindricalConstraint,
        "Velocity",
        "With a Motor ActuatorType, how fast it slides Attachment1 along the axis, in studs a second.",
    ),
    (
        ClassName::CylindricalConstraint,
        "MotorMaxForce",
        "With a Motor ActuatorType, the most force it slides with. `math.huge` has no limit.",
    ),
    (
        ClassName::CylindricalConstraint,
        "TargetPosition",
        "With a Servo ActuatorType, how far along the axis it slides Attachment1 to and holds, in studs.",
    ),
    (
        ClassName::CylindricalConstraint,
        "Speed",
        "With a Servo ActuatorType, the fastest it slides toward TargetPosition, in studs a second.",
    ),
    (
        ClassName::CylindricalConstraint,
        "ServoMaxForce",
        "With a Servo ActuatorType, the most force it slides and holds with.",
    ),
    (
        ClassName::CylindricalConstraint,
        "AngularLimitsEnabled",
        "On, it only turns between LowerAngle and UpperAngle.",
    ),
    (
        ClassName::CylindricalConstraint,
        "LowerAngle",
        "While AngularLimitsEnabled is on, the farthest it turns one way, in degrees.",
    ),
    (
        ClassName::CylindricalConstraint,
        "UpperAngle",
        "While AngularLimitsEnabled is on, the farthest it turns the other way, in degrees.",
    ),
    (
        ClassName::CylindricalConstraint,
        "AngularActuatorType",
        "What turns it around the axis: `Enum.ActuatorType.None` (free), `Motor` (spins at AngularVelocity) or `Servo` (turns to TargetAngle).",
    ),
    (
        ClassName::CylindricalConstraint,
        "AngularVelocity",
        "With a Motor actuator, how fast it turns Attachment1 around the axis, in radians a second.",
    ),
    (
        ClassName::CylindricalConstraint,
        "MotorMaxTorque",
        "With a Motor actuator, the most torque it turns with. `math.huge` has no limit.",
    ),
    (
        ClassName::CylindricalConstraint,
        "TargetAngle",
        "With a Servo actuator, the angle it turns Attachment1 to and holds, in degrees.",
    ),
    (
        ClassName::CylindricalConstraint,
        "AngularSpeed",
        "With a Servo actuator, the fastest it turns toward TargetAngle, in radians a second.",
    ),
    (
        ClassName::CylindricalConstraint,
        "ServoMaxTorque",
        "With a Servo actuator, the most torque it turns and holds with. Too little, and a heavy load drags it.",
    ),
    (
        ClassName::SpringConstraint,
        "Attachment0",
        "The Attachment on the first part. The joint's axis is this attachment's X axis (its `Axis`).",
    ),
    (
        ClassName::SpringConstraint,
        "Attachment1",
        "The Attachment on the second part, held to Attachment0.",
    ),
    (
        ClassName::SpringConstraint,
        "Enabled",
        "On (the default), it joins the attachments' parts. Off, it holds nothing.",
    ),
    (
        ClassName::SpringConstraint,
        "Visible",
        "On, the game draws it in Color: a coil for a spring, a line for a rope or rod, an axle or ball for the others. Off by default; the editor's Constraints overlay always shows it.",
    ),
    (
        ClassName::SpringConstraint,
        "Color",
        "The color it's drawn in while Visible.",
    ),
    (
        ClassName::SpringConstraint,
        "Stiffness",
        "How hard it pulls or pushes toward FreeLength: force per stud it's stretched or squashed.",
    ),
    (
        ClassName::SpringConstraint,
        "Damping",
        "How much it resists moving, so it stops bouncing: force per stud a second of stretching.",
    ),
    (
        ClassName::SpringConstraint,
        "FreeLength",
        "The length it settles at with nothing pulling on it, in studs.",
    ),
    (
        ClassName::SpringConstraint,
        "MinLength",
        "The shortest it can be squashed, in studs.",
    ),
    (
        ClassName::SpringConstraint,
        "MaxLength",
        "The longest it can be stretched, in studs.",
    ),
    (
        ClassName::SpringConstraint,
        "Coils",
        "How many turns the drawn coil has.",
    ),
    (
        ClassName::SpringConstraint,
        "Radius",
        "The drawn coil's radius, in studs.",
    ),
    (
        ClassName::SpringConstraint,
        "Thickness",
        "How thick it's drawn while Visible, in studs.",
    ),
    (
        ClassName::RopeConstraint,
        "Attachment0",
        "The Attachment on the first part. The joint's axis is this attachment's X axis (its `Axis`).",
    ),
    (
        ClassName::RopeConstraint,
        "Attachment1",
        "The Attachment on the second part, held to Attachment0.",
    ),
    (
        ClassName::RopeConstraint,
        "Enabled",
        "On (the default), it joins the attachments' parts. Off, it holds nothing.",
    ),
    (
        ClassName::RopeConstraint,
        "Visible",
        "On, the game draws it in Color: a coil for a spring, a line for a rope or rod, an axle or ball for the others. Off by default; the editor's Constraints overlay always shows it.",
    ),
    (
        ClassName::RopeConstraint,
        "Color",
        "The color it's drawn in while Visible.",
    ),
    (
        ClassName::RopeConstraint,
        "Length",
        "The farthest apart the attachments can be, in studs. Closer, the rope is slack.",
    ),
    (
        ClassName::RopeConstraint,
        "WinchEnabled",
        "On, a winch reels the rope in or out toward WinchTarget, starting from Length.",
    ),
    (
        ClassName::RopeConstraint,
        "WinchTarget",
        "The length the winch reels the rope to, in studs.",
    ),
    (
        ClassName::RopeConstraint,
        "WinchSpeed",
        "How fast the winch reels, in studs a second.",
    ),
    (
        ClassName::RopeConstraint,
        "WinchForce",
        "The most the winch pulls with: a heavier load stops it reeling in (it still holds).",
    ),
    (
        ClassName::RopeConstraint,
        "Thickness",
        "How thick it's drawn while Visible, in studs.",
    ),
    (
        ClassName::RodConstraint,
        "Attachment0",
        "The Attachment on the first part. The joint's axis is this attachment's X axis (its `Axis`).",
    ),
    (
        ClassName::RodConstraint,
        "Attachment1",
        "The Attachment on the second part, held to Attachment0.",
    ),
    (
        ClassName::RodConstraint,
        "Enabled",
        "On (the default), it joins the attachments' parts. Off, it holds nothing.",
    ),
    (
        ClassName::RodConstraint,
        "Visible",
        "On, the game draws it in Color: a coil for a spring, a line for a rope or rod, an axle or ball for the others. Off by default; the editor's Constraints overlay always shows it.",
    ),
    (
        ClassName::RodConstraint,
        "Color",
        "The color it's drawn in while Visible.",
    ),
    (
        ClassName::RodConstraint,
        "Length",
        "How far apart the attachments are held, in studs.",
    ),
    (
        ClassName::RodConstraint,
        "Thickness",
        "How thick it's drawn while Visible, in studs.",
    ),
    (
        ClassName::Camera,
        "Position",
        "Where the camera is, in studs.",
    ),
    (
        ClassName::Camera,
        "Orientation",
        "Which way it looks, in degrees like a Part's: X looks up (positive) and down, Y turns left (positive) and right. Z is ignored.",
    ),
    (
        ClassName::Camera,
        "FieldOfView",
        "How much it sees from top to bottom, in degrees (1 to 120; 50 to start with). Smaller zooms in.",
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
    (
        ClassName::LinearVelocity,
        "Attachment0",
        "The Attachment it pushes or turns, on the part it moves (with everything welded to it).",
    ),
    (
        ClassName::LinearVelocity,
        "Attachment1",
        "The Attachment it measures from with RelativeTo Attachment1, or follows in TwoAttachment Mode. Optional.",
    ),
    (
        ClassName::LinearVelocity,
        "Enabled",
        "On (the default), it acts. Off, it does nothing.",
    ),
    (
        ClassName::LinearVelocity,
        "VectorVelocity",
        "The velocity it keeps Attachment0's part at, in studs a second, in RelativeTo's axes. Zero holds the part still in the air.",
    ),
    (
        ClassName::LinearVelocity,
        "MaxForce",
        "The most force it pushes with, gravity included. A part too heavy for it sinks; `math.huge` has no limit.",
    ),
    (
        ClassName::LinearVelocity,
        "RelativeTo",
        "Which axes its vector is in: `Enum.ActuatorRelativeTo.World`, `Attachment0` (turning with the part) or `Attachment1`. World by default.",
    ),
    (
        ClassName::AngularVelocity,
        "Attachment0",
        "The Attachment it pushes or turns, on the part it moves (with everything welded to it).",
    ),
    (
        ClassName::AngularVelocity,
        "Attachment1",
        "The Attachment it measures from with RelativeTo Attachment1, or follows in TwoAttachment Mode. Optional.",
    ),
    (
        ClassName::AngularVelocity,
        "Enabled",
        "On (the default), it acts. Off, it does nothing.",
    ),
    (
        ClassName::AngularVelocity,
        "AngularVelocity",
        "How fast it spins Attachment0's part around each axis, in radians a second, in RelativeTo's axes.",
    ),
    (
        ClassName::AngularVelocity,
        "MaxTorque",
        "The most torque it turns with. `math.huge` has no limit.",
    ),
    (
        ClassName::AngularVelocity,
        "RelativeTo",
        "Which axes its vector is in: `Enum.ActuatorRelativeTo.World`, `Attachment0` (turning with the part) or `Attachment1`. World by default.",
    ),
    (
        ClassName::VectorForce,
        "Attachment0",
        "The Attachment it pushes or turns, on the part it moves (with everything welded to it).",
    ),
    (
        ClassName::VectorForce,
        "Attachment1",
        "The Attachment it measures from with RelativeTo Attachment1, or follows in TwoAttachment Mode. Optional.",
    ),
    (
        ClassName::VectorForce,
        "Enabled",
        "On (the default), it acts. Off, it does nothing.",
    ),
    (
        ClassName::VectorForce,
        "Force",
        "The force it pushes with every step. Its part's mass times `workspace.Gravity` upward makes the part hover.",
    ),
    (
        ClassName::VectorForce,
        "RelativeTo",
        "Which axes its vector is in: `Enum.ActuatorRelativeTo.World`, `Attachment0` (turning with the part) or `Attachment1`. Attachment0 by default.",
    ),
    (
        ClassName::VectorForce,
        "ApplyAtCenterOfMass",
        "On, it pushes at the center of mass, so it never turns the part. Off (the default), it pushes at Attachment0.",
    ),
    (
        ClassName::Torque,
        "Attachment0",
        "The Attachment it pushes or turns, on the part it moves (with everything welded to it).",
    ),
    (
        ClassName::Torque,
        "Attachment1",
        "The Attachment it measures from with RelativeTo Attachment1, or follows in TwoAttachment Mode. Optional.",
    ),
    (
        ClassName::Torque,
        "Enabled",
        "On (the default), it acts. Off, it does nothing.",
    ),
    (
        ClassName::Torque,
        "Torque",
        "The torque it turns its part with every step, around RelativeTo's axes.",
    ),
    (
        ClassName::Torque,
        "RelativeTo",
        "Which axes its vector is in: `Enum.ActuatorRelativeTo.World`, `Attachment0` (turning with the part) or `Attachment1`. Attachment0 by default.",
    ),
    (
        ClassName::AlignPosition,
        "Attachment0",
        "The Attachment it pushes or turns, on the part it moves (with everything welded to it).",
    ),
    (
        ClassName::AlignPosition,
        "Attachment1",
        "The Attachment it measures from with RelativeTo Attachment1, or follows in TwoAttachment Mode. Optional.",
    ),
    (
        ClassName::AlignPosition,
        "Enabled",
        "On (the default), it acts. Off, it does nothing.",
    ),
    (
        ClassName::AlignPosition,
        "Mode",
        "`Enum.PositionAlignmentMode.TwoAttachment` (the default) moves Attachment0 to Attachment1, following it; `OneAttachment` moves it to Position.",
    ),
    (
        ClassName::AlignPosition,
        "Position",
        "Where it moves Attachment0 in OneAttachment Mode, in the world.",
    ),
    (
        ClassName::AlignPosition,
        "MaxForce",
        "The most force it pushes with, gravity included. `math.huge` has no limit.",
    ),
    (
        ClassName::AlignPosition,
        "MaxVelocity",
        "The fastest it moves Attachment0 toward its target, in studs a second.",
    ),
    (
        ClassName::AlignPosition,
        "Responsiveness",
        "How quickly it gets there, from 5 to 200: it closes most of the gap in about 1/Responsiveness seconds, smoothly, without overshooting.",
    ),
    (
        ClassName::AlignPosition,
        "RigidityEnabled",
        "On, it gets there as fast as MaxForce and MaxVelocity allow, ignoring Responsiveness.",
    ),
    (
        ClassName::AlignOrientation,
        "Attachment0",
        "The Attachment it pushes or turns, on the part it moves (with everything welded to it).",
    ),
    (
        ClassName::AlignOrientation,
        "Attachment1",
        "The Attachment it measures from with RelativeTo Attachment1, or follows in TwoAttachment Mode. Optional.",
    ),
    (
        ClassName::AlignOrientation,
        "Enabled",
        "On (the default), it acts. Off, it does nothing.",
    ),
    (
        ClassName::AlignOrientation,
        "Mode",
        "`Enum.OrientationAlignmentMode.TwoAttachment` (the default) turns Attachment0 to face the way Attachment1 does; `OneAttachment` turns it to CFrame's rotation.",
    ),
    (
        ClassName::AlignOrientation,
        "CFrame",
        "The rotation it turns Attachment0 to in OneAttachment Mode, in the world. Its position is ignored.",
    ),
    (
        ClassName::AlignOrientation,
        "MaxTorque",
        "The most torque it turns with. `math.huge` has no limit.",
    ),
    (
        ClassName::AlignOrientation,
        "MaxAngularVelocity",
        "The fastest it turns Attachment0 toward its target, in radians a second.",
    ),
    (
        ClassName::AlignOrientation,
        "Responsiveness",
        "How quickly it gets there, from 5 to 200: it closes most of the turn in about 1/Responsiveness seconds, smoothly, without overshooting.",
    ),
    (
        ClassName::AlignOrientation,
        "RigidityEnabled",
        "On, it turns there as fast as MaxTorque and MaxAngularVelocity allow, ignoring Responsiveness.",
    ),
];

const BREAK_FORCE_DOC: &str = "How much force (mass times studs a second squared: a 1×1×1 plastic block weighs 196.2) holds it together before it breaks, turns off and fires Broken. 0 (the default) never breaks. A weld with a BreakForce or BreakTorque holds its parts as two bodies joined by a joint, so its parts aren't one assembly.";
const BREAK_TORQUE_DOC: &str = "How much torque holds it together before it breaks, turns off and fires Broken; a weld's is measured halfway between its parts. 0 (the default) never breaks.";
const BROKEN_DOC: &str = "Fires once when the force or torque holding it together goes past its BreakForce or BreakTorque. It's already off (Enabled is false). The handler gets no arguments.";

/// What each event means, including what its handler receives. A test
/// checks every event in `core` is here.
pub(super) const EVENT_DOCS: &[(ClassName, &str, &str)] = &[
    (ClassName::WeldConstraint, "Broken", BROKEN_DOC),
    (ClassName::HingeConstraint, "Broken", BROKEN_DOC),
    (ClassName::BallSocketConstraint, "Broken", BROKEN_DOC),
    (ClassName::PrismaticConstraint, "Broken", BROKEN_DOC),
    (ClassName::CylindricalConstraint, "Broken", BROKEN_DOC),
    (ClassName::SpringConstraint, "Broken", BROKEN_DOC),
    (ClassName::RopeConstraint, "Broken", BROKEN_DOC),
    (ClassName::RodConstraint, "Broken", BROKEN_DOC),
    (
        ClassName::Fracture,
        "Shattered",
        "Fires once when the part is replaced by its pieces, hit hard enough or by `Shatter`. The handler gets no arguments; the part has left the game (its Parent is nil) and the pieces are in a Model named after it.",
    ),
    (
        ClassName::Part,
        "Touched",
        "Fires when another part starts touching this one, once per new contact (not every frame while they touch). The handler gets the other part: `part.Touched:Connect(function(otherPart) ... end)`.",
    ),
    (
        ClassName::Part,
        "TouchEnded",
        "Fires when another part stops touching this one: once they've been apart for a few physics steps, or at once when one of them can't touch the other any more (`CanTouch` off, or set not to collide). The handler gets the other part.",
    ),
    (
        ClassName::MeshPart,
        "Touched",
        "Fires when another part starts touching this one, once per new contact (not every frame while they touch). The handler gets the other part: `part.Touched:Connect(function(otherPart) ... end)`.",
    ),
    (
        ClassName::MeshPart,
        "TouchEnded",
        "Fires when another part stops touching this one: once they've been apart for a few physics steps, or at once when one of them can't touch the other any more (`CanTouch` off, or set not to collide). The handler gets the other part.",
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
        ClassName::BindableEvent,
        "Event",
        "Fires when a script on the same side calls `Fire`. The handler gets the same arguments (tables aren't copied).",
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
        ClassName::ThirdPersonController,
        "Seated",
        "Fires when the character sits down (true and the Seat) or stands up (false and the Seat it left).",
    ),
    (
        ClassName::FirstPersonController,
        "Seated",
        "Fires when the character sits down (true and the Seat) or stands up (false and the Seat it left).",
    ),
    (
        ClassName::ThirdPersonController,
        "Landed",
        "Fires when the character lands, with how fast it was falling in studs a second. A Health in its Model with FallDamage on takes its damage first.",
    ),
    (
        ClassName::FirstPersonController,
        "Landed",
        "Fires when the character lands, with how fast it was falling in studs a second. A Health in its Model with FallDamage on takes its damage first.",
    ),
    (
        ClassName::Health,
        "HealthChanged",
        "Fires whenever Health changes: damage, Heal, regeneration or a script setting it. The handler gets the new health and the old.",
    ),
    (
        ClassName::Health,
        "Damaged",
        "Fires when it takes damage, after HealthChanged. The handler gets how much health it lost and what did it: the `source` given to TakeDamage, the DamageZone, or nil (a fall, or no source).",
    ),
    (
        ClassName::Health,
        "Died",
        "Fires once when Health reaches 0, after HealthChanged, however many hits landed that frame. Nothing else happens to the Model unless it has a Humanoid: scripts decide what dying does. It can fire again only after a script sets Health above 0.",
    ),
    (
        ClassName::Humanoid,
        "MoveToFinished",
        "Fires when a MoveTo ends. The handler gets true if the character got there, or false if it couldn't (no path on the navigation mesh, 8 seconds passed, or it died).",
    ),
    (
        ClassName::Humanoid,
        "StateChanged",
        "Fires when the character's state changes. The handler gets the old and new `Enum.HumanoidStateType`: Running, Jumping, Freefall, Landed (for the frame it lands), Seated, Ragdoll or Dead. Swimming is reserved.",
    ),
    (
        ClassName::Humanoid,
        "Died",
        "Fires once when the character's Health reaches 0, after the Health's own Died and StateChanged to Dead. By then the controller has stopped and RagdollOnDeath and BreakJointsOnDeath are done.",
    ),
    (
        ClassName::DataLayerService,
        "LayerChanged",
        "Fires with the layer's name and whether it's now on, after SetLayerActive switched it (or a save slot loaded with it the other way). On both sides: a player's device hears it once the change and the parts arrive.",
    ),
    (
        ClassName::TextChatService,
        "MessageReceived",
        "Fires on each device as a chat message arrives there, with the message: `message.Text`, and `message.Player` (nil for a system message). On both sides, after the server's OnIncomingMessage.",
    ),
    (
        ClassName::Players,
        "PlayerAdded",
        "Fires with the Player when someone joins, after they're put on a team and before their first character spawns.",
    ),
    (
        ClassName::Players,
        "PlayerRemoving",
        "Fires with the Player when someone leaves a hosted game, just before their Player (and character) are destroyed.",
    ),
    (
        ClassName::Players,
        "PlayerFlagged",
        "Scripts only: fires with the Player and why (a string) when the server catches what their device sent breaking a rule, such as sending far more than a game client does or tables nested too deep. At most once a second for each reason. The server kicks a player flagged too often in a minute on its own (`axcenity-server --kick-flags`); a script may kick sooner or note it.",
    ),
    (
        ClassName::Player,
        "CharacterAdded",
        "Fires with the character Model once it's in the Workspace: when the player first gets one, after each respawn and after `LoadCharacter`.",
    ),
    (
        ClassName::Player,
        "CharacterRemoving",
        "Fires with the old character Model just before it's destroyed to make way for a new one. It's already out of the Workspace (its Parent is nil).",
    ),
    (
        ClassName::ProximityPrompt,
        "Triggered",
        "Fires with the Player when the action is pressed on a showing prompt (HoldDuration 0) or held for HoldDuration. Once per press: hold on and it doesn't fire again.",
    ),
    (
        ClassName::ProximityPrompt,
        "TriggerEnded",
        "Fires with the Player when the action that triggered it is let go, or the prompt hides.",
    ),
    (
        ClassName::ProximityPrompt,
        "PromptShown",
        "Fires when the prompt starts showing.",
    ),
    (
        ClassName::ProximityPrompt,
        "PromptHidden",
        "Fires when the prompt stops showing: out of reach, out of sight, Enabled off, or another is nearer.",
    ),
    (
        ClassName::ProximityPrompt,
        "PromptButtonHoldBegan",
        "Fires with the Player when the action is pressed on a showing prompt whose HoldDuration is above 0.",
    ),
    (
        ClassName::ProximityPrompt,
        "PromptButtonHoldEnded",
        "Fires with the Player when a hold ends: let go early, the prompt hid, or HoldDuration was reached (just before Triggered).",
    ),
    (
        ClassName::ClickDetector,
        "MouseClick",
        "Fires with the Player when the left button is pressed and let go on the part, from within MaxActivationDistance.",
    ),
    (
        ClassName::ClickDetector,
        "RightMouseClick",
        "Fires with the Player when the right button is pressed and let go on the part, from within MaxActivationDistance.",
    ),
    (
        ClassName::ClickDetector,
        "MouseHoverEnter",
        "Fires with the Player when the mouse starts pointing at the part from within reach.",
    ),
    (
        ClassName::ClickDetector,
        "MouseHoverLeave",
        "Fires with the Player when the mouse stops pointing at the part, or goes out of reach.",
    ),
    (
        ClassName::Tool,
        "Equipped",
        "Fires when it goes into the hand: Equip, the hotbar, or a save slot loading with it in hand.",
    ),
    (
        ClassName::Tool,
        "Unequipped",
        "Fires when it leaves the hand: put away, another equipped, dropped or removed.",
    ),
    (
        ClassName::Tool,
        "Activated",
        "Fires when the Primary action (the left mouse button, or RT) is pressed while it's in the player character's hand, such as to swing a sword.",
    ),
    (
        ClassName::Tool,
        "Deactivated",
        "Fires when the Primary action is let go after Activated, or the Tool leaves the hand first.",
    ),
    (
        ClassName::Ability,
        "Activated",
        "Fires on the server and its players' devices as it activates, with the Player who asked (nil when a Script activated it). Do what it does here, such as the damage.",
    ),
    (
        ClassName::Ability,
        "Ended",
        "Fires as it stops being active, with true when Cancel ended it early. An Ability with no Duration ends as it activates.",
    ),
    (
        ClassName::Ability,
        "CooldownChanged",
        "Fires as its cooldown starts, with Cooldown, and as it ends, with 0.",
    ),
    (
        ClassName::SplineFollower,
        "Reached",
        "Fires as it gets to an end of its spline, with \"End\" or \"Start\": a Loop as it comes round to the start again, a PingPong as it turns back, and a Once as it stops.",
    ),
    (
        ClassName::NavigationAgent,
        "PlayerSensed",
        "Fires when the agent starts seeing or hearing the player character. The handler gets the sense: \"Sight\" or \"Hearing\".",
    ),
    (
        ClassName::Sequence,
        "Completed",
        "Fires once when it reaches its end, or is skipped there. The handler gets no arguments.",
    ),
    (
        ClassName::Sequence,
        "EventReached",
        "Fires with an event's name as it passes the event on its Events track. Events skipped over aren't fired.",
    ),
    (
        ClassName::Animation,
        "Completed",
        "Fires when the Animation, played by AutoPlay, reaches the end of a clip that doesn't loop. The handler gets no arguments.",
    ),
    (
        ClassName::Animation,
        "AnimationEvent",
        "Fires when the Animation, played by AutoPlay, passes one of the clip's events. The handler gets the event's name.",
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
            "The service that holds server scripts (`scripts/*.server.lua`). Players' devices never get what's inside."
        }
        ClassName::ServerStorage => {
            "The service for things only the server uses, such as Models a Script clones into the Workspace. It's saved with the world, and players' devices never get what's inside: to them it's empty."
        }
        ClassName::StarterPlayer => {
            "The service with what each player starts with: StarterPlayerScripts, and the StarterCharacter each character is a copy of."
        }
        ClassName::Teams => "The service that holds the Teams players can be on.",
        ClassName::SoundService => {
            "The service that holds the SoundGroups, the game's audio buses. Its groups' volumes are saved with the world in save slots. When a group is UserAdjustable, the game has an AudioSettings ScreenGui in the Workspace (made when it starts, hidden until a script sets its Enabled) with a slider for UserGameSettings.MasterVolume and one for each of those groups."
        }
        ClassName::DataLayerService => {
            "The service that holds the project's DataLayers (roadmap R16). Every level shares them. In a game, a switched-off layer's Instances are set aside inside the layer: not simulated, drawn or sent to players. SetLayerActive (Scripts only) switches a layer, IsLayerActive reads it and LayerChanged reports it on every device. A loaded level's own members of a switched-off layer are set aside too, and save slots keep each layer's state."
        }
        ClassName::DataLayer => {
            "A data layer (roadmap R16): the Instances tagged with its Name (CollectionService), wherever they are in the tree. The editor's Layers panel shows, hides and locks them together; renaming the layer renames the tag."
        }
        ClassName::TextChatService => {
            "The default chat (Phase 67): a chat window at the bottom left that `/` opens, styled with `TextChatService` rules (`TextChatService::key` styles the names). SendMessage sends from a LocalScript; on the server, OnIncomingMessage sees every message and returns its text, changed or not, or nil to drop it; DisplaySystemMessage shows a message from the game. MessageReceived fires on each device as a message arrives. ChatEnabled off hides the window."
        }
        ClassName::SoundGroup => {
            "An audio bus: the Sounds whose SoundGroup is it, and the SoundGroups inside it, are mixed together at its Volume, through the sound effects inside it. Put them in SoundService, such as Master with Music and SFX inside."
        }
        ClassName::ReverbSoundEffect => {
            "Inside a Sound or SoundGroup: makes it sound like a room, with DecayTime, Density, DryLevel and WetLevel."
        }
        ClassName::EqualizerSoundEffect => {
            "Inside a Sound or SoundGroup: raises or lowers its lows, mids and highs (LowGain, MidGain, HighGain, around MidRange)."
        }
        ClassName::CompressorSoundEffect => {
            "Inside a Sound or SoundGroup: turns down what's louder than Threshold by Ratio, so loud and quiet sit closer together."
        }
        ClassName::EchoSoundEffect => {
            "Inside a Sound or SoundGroup: repeats it every Delay seconds, each repeat Feedback decibels quieter."
        }
        ClassName::DistortionSoundEffect => {
            "Inside a Sound or SoundGroup: overdrives it, by Level."
        }
        ClassName::PitchShiftSoundEffect => {
            "Inside a Sound or SoundGroup: raises or lowers its pitch by Octave without changing its speed."
        }
        ClassName::LowPassSoundEffect => {
            "Inside a Sound or SoundGroup: lets through only what's below Cutoff, so it sounds muffled, as through a wall or under a pause menu."
        }
        ClassName::HighPassSoundEffect => {
            "Inside a Sound or SoundGroup: lets through only what's above Cutoff, so it sounds thin, as through a radio."
        }
        ClassName::GraphicsVolume => {
            "A Part that changes the look while the camera is inside it, fading in over BlendDistance studs outside it and out again on leaving. The effects inside it replace that part of the world's look: a ColorCorrectionEffect, an ExposureEffect, a FogEffect, an Atmosphere (the sky, the sun's color and height fog) or a BloomEffect; what it doesn't hold stays as it is. Where volumes overlap, the higher Priority wins. Invisible, anchored and walked through (CanCollide off) by default. Everything a Part has, and Enabled, BlendDistance and Priority."
        }
        ClassName::ExposureEffect => {
            "Inside a GraphicsVolume: the exposure while the camera is in the volume, in place of Lighting's Exposure."
        }
        ClassName::FogEffect => {
            "Inside a GraphicsVolume: distance fog from Start to End studs in Color while the camera is in the volume, in place of the world's fog. With a Density above 0, dust or mist filling the volume's box instead, seen from anywhere and lit by the sun (shafts of light through windows and gaps); with Interior on, the volume clears the outdoor fog inside it."
        }
        ClassName::BloomEffect => {
            "Inside a GraphicsVolume: how bright light blooms while the camera is in the volume, in place of Lighting's BloomIntensity and BloomThreshold."
        }
        ClassName::ReverbZone => {
            "A Part the camera walks into: while the camera is inside it, its ReverbType blends onto every spatial Sound over FadeTime seconds, and out again on leaving. Where zones overlap, the smallest wins. Anchored, mostly see-through and walked through (CanCollide off) by default. Everything a Part has, and ReverbType and FadeTime."
        }
        ClassName::Team => {
            "A side players can be on, in Teams. Its players spawn on SpawnLocations of its TeamColor; players joining go on the AutoAssignable team with the fewest players. `GetPlayers` lists them."
        }
        ClassName::SpawnLocation => {
            "A Part characters spawn on: a player's character goes on top of an Enabled SpawnLocation of their team's TeamColor (or a Neutral one when the team has none), chosen at random. With none, it goes at the origin and Output warns. Anchored and 12 × 1 × 12 by default. Everything a Part has, and Enabled, Neutral, TeamColor and Duration."
        }
        ClassName::ForceField => {
            "Keeps the Health of the Model it's in from losing any: TakeDamage, DamageZones and falls do nothing. A SpawnLocation puts one in each character it spawns, for its Duration seconds."
        }
        ClassName::StarterPlayerScripts => {
            "Holds the LocalScripts (`scripts/*.client.lua`), which run on the player's side."
        }
        ClassName::Part => "A solid shape in the world.",
        ClassName::Seat => {
            "A Part that characters sit on. A character that touches it while it's empty and not Disabled sits down: it's welded to the seat (so it rides along on a moving cart), plays its Animator's `Sit` state if there is one, and ignores move keys. Jump stands it up in front of the seat. Everything a Part has, and Occupant, Disabled and SeatOffset."
        }
        ClassName::VehicleSeat => {
            "A Seat that drives the car it's welded into: the body made of every part welded to it, the chassis. While someone sits on it, the Move action sets Throttle and Steer, and the camera follows the car until they get out with Jump. The car rides on the VehicleWheels parented to its parts. Everything a Seat has, and MaxSpeed, Torque, TurnSpeed, SteerAngle, Throttle, Steer and HandBrake."
        }
        ClassName::IKControl => {
            "Inverse kinematics during the game, for the skinned MeshPart it's in (or its Animator). After the Animator and before drawing, it turns the bones from ChainRoot down to EndEffector toward its Target, by its Type. Weight and Enabled fade in and out over SmoothTime."
        }
        ClassName::Ragdoll => {
            "A ragdoll for the skinned MeshPart it's in: a capsule Part per main bone, named after the bone, and a BallSocketConstraint per joint, which Build (in its Properties) makes from the skeleton with a humanoid preset's limits. While Enabled, physics moves the capsules and they pose the bones; otherwise they follow the animation and physics leaves them out."
        }
        ClassName::Health => {
            "How much a Model can take: Health out of MaxHealth, put in any Model. TakeDamage, DamageZones and falls (with FallDamage on) lower it; Heal and regeneration (RegenRate, after RegenDelay seconds without damage) raise it. At 0 it fires Died once and the Model is left for scripts to handle."
        }
        ClassName::ProximityPrompt => {
            "A prompt to do something, under a part or an Attachment. When the player character is within MaxActivationDistance (and, with RequiresLineOfSight, nothing solid is between them), the nearest prompt for its Action shows ActionText · ObjectText over it with the bound key or controller button, and holding that action for HoldDuration seconds fires Triggered. Styled in CSS as `ProximityPrompt`, with `::key` and `::ring`."
        }
        ClassName::ClickDetector => {
            "Makes the part it's in clickable. Pointing at the part from within MaxActivationDistance fires MouseHoverEnter and shows CursorIcon; a left click fires MouseClick and a right click RightMouseClick. Clicks the game UI takes don't count."
        }
        ClassName::Tool => {
            "Something a character carries: a Model of parts with a Handle. Touching a dropped Tool puts it in the character's Inventory, if there's room; equipping it holds its Grip in the right hand (a skinned character's right hand bone, a part named RightHand, or beside the body). Activated fires on the Primary action while it's in hand. Drop with the Drop action (Backspace, or Y) or tool:Drop()."
        }
        ClassName::Inventory => {
            "The Tools a character carries, put in the character's Model: up to Capacity Tools as its children, in hotbar order, and Equipped, the one in hand. The default hotbar shows them; number keys, the mouse wheel (while one is in hand) and the controller bumpers pick one. Styled in CSS as `Inventory` (the bar), `Tool` (each slot, `Tool:checked` in hand) and `Tool::key` (its number). Save slots keep it with the world."
        }
        ClassName::Humanoid => {
            "A Roblox-familiar front for a character, in its Model. It finds the Model's character controller, Health, Inventory and Animator, making each one that's missing during Play, and its WalkSpeed, RunSpeed, JumpHeight, Sit, PlatformStand, AutoRotate and MoveDirection are the controller's, and Health, MaxHealth, DisplayName and HealthDisplayDistance the Health's: there's one value, so the pieces work the same with or without it. It stores only BreakJointsOnDeath and RagdollOnDeath. Its Animator plays its Idle, Walk, Run, Jump and Fall states, if it has them."
        }
        ClassName::DamageZone => {
            "A Part that hurts: every Health whose Model has a part inside it loses DamagePerSecond a second, with the zone as the source. Anchored, half see-through and walked through (CanCollide off) by default. Everything a Part has, and DamagePerSecond."
        }
        ClassName::UnionOperation => {
            "Parts combined into one solid by Union or Intersect (Model ▸ Solid Modeling), or by `UnionAsync`, `SubtractAsync` or `IntersectAsync`. Drawn and collided as its model, which the editor saves as `<name>.union.glb` in `assets/models`; each face keeps the color and material of the part it came from unless UsePartColor is on. The editor keeps the parts it was made of inside it, hidden, so Separate can give them back."
        }
        ClassName::NegateOperation => {
            "A Part made negative by Negate (Model ▸ Solid Modeling): shown pink and see-through, and when it's unioned with other parts it cuts itself out of them, as a doorway out of a wall. Negate it again to turn it back into a Part. Everything a Part has."
        }
        ClassName::Fracture => {
            "Breaks the Part or MeshPart it's in into pieces. Generate (in its Properties) cuts the part into Pieces convex pieces by Seed and Pattern and saves them as `<name>.fracture.glb`. During Play, something hitting the part faster than Threshold, or `Shatter`, replaces the part with its pieces, which fly from the point and fade out after DebrisLifetime."
        }
        ClassName::VehicleWheel => {
            "A wheel of the car whose part it's parented to: a spring and a ray to the ground, so no wheel collider touches it. Position and Orientation are relative to the part, as an Attachment's. Not drawn; its WheelPart is moved to it."
        }
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
        ClassName::RemoteFunction => {
            "Asks the server and waits for the answer: a LocalScript's `InvokeServer(...)` pauses until the server's `OnServerInvoke` returns, and returns what it returned. An error there is raised in the LocalScript. Keep it in ReplicatedStorage."
        }
        ClassName::BindableEvent => {
            "An event scripts on the same side fire for each other: `Fire(...)` fires `Event` with the same values. It doesn't cross between Scripts and LocalScripts."
        }
        ClassName::BindableFunction => {
            "A function scripts on the same side call for each other: `Invoke(...)` calls `OnInvoke` and returns what it returns. It doesn't cross between Scripts and LocalScripts."
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
            "A light that shines in all directions from just above its parent part. Anywhere else (in the Workspace, a Model or a Folder) it needs no part: it shines from its own Position, as in Unreal, and the editor shows it as an icon you can drag."
        }
        ClassName::SpotLight => {
            "A light that shines in a cone from just above its parent part. Anywhere else it needs no part: it shines from its own Position, its Direction turned by its Orientation, and the editor shows it as an icon you can drag."
        }
        ClassName::RectLight => {
            "A light that shines from a rectangle Width by Height studs, out of its front only, like a window or a softbox: on the side of its parent part its Direction points out of, or anywhere else from its own Position, turned by its Orientation (the editor shows it as an icon you can drag)."
        }
        ClassName::ReflectionProbe => {
            "Inside a Part: shiny surfaces in the part's box reflect the world as the editor baked it from the part's middle (Lighting > Bake Probes), lined up with the box's walls, in place of the sky. Overlapping probes blend, the smaller one winning, and each fades into what's around it over its BlendDistance. Changing the world doesn't change a bake until it's baked again. Drawn at Medium reflection quality and above."
        }
        ClassName::LightProbeVolume => {
            "Inside a Part: a grid of light probes filling the part's box, baked in the editor (Lighting > Bake Probes) with the light from the sun, lamps and sky and what it bounces off. Everything in the box, still or moving, is lit by the probes around it in place of the sky's even light, so a part beside a red wall picks up its red. Overlapping volumes blend, the smaller one winning, and each fades into what's around it over its BlendDistance. Changing the world doesn't change a bake until it's baked again; the Explorer warns when it's out of date."
        }
        ClassName::Atmosphere => {
            "Inside Lighting: the sky is drawn as sunlight scattered by air in place of the sky colors: blue by day, red and orange toward sunset, dark and starry at night as ClockTime goes round. The air also colors the sun's light and lights and is reflected by parts. Far parts fade into its fog, the sky's color around the horizon, thickest low down; at Ultra graphics the fog shows the sun's shadows (sun shafts) and lamps' light. Lighting's own FogStart and FogEnd aren't used while it's there. A SkyImage is still drawn in its place, but the sun's light is colored by the air."
        }
        ClassName::Clouds => {
            "Inside Lighting: a layer of cloud high in the sky (800 to 1200 studs up), lit by the sun and sky, whose shadows fall on the world beneath. They drift at their own Speed toward their Direction, and workspace.GlobalWind carries them too; their shadows move with them. The player's graphics settings choose how finely it's drawn (UserGameSettings.CloudQuality); at Low it's a flat sheet."
        }
        ClassName::Weather => {
            "Inside Lighting: rain or snow falling around the camera, and how wet and snow-covered surfaces are (parts, models and terrain). Wet surfaces are darker and glossier; snow covers the tops of things but not walls. workspace.GlobalWind blows the rain and snow. Every change eases in over TransitionTime. With Precipitation None and no Wetness or SnowCover it costs nothing."
        }
        ClassName::ColorCorrectionEffect => {
            "Inside Lighting: changes the colors of the finished picture, after tone mapping. Brightness, Contrast, Saturation and TintColor come first, then the LookupTable, Posterize and the Palette. Several in Lighting add up (their tints multiply). Inside a GraphicsVolume, those in the volume replace Lighting's while the camera is there."
        }
        ClassName::DepthOfFieldEffect => {
            "Inside Lighting: blurs what's nearer or farther than the distance in focus, as a camera's lens does: sharp from FocusDistance - InFocusRadius to FocusDistance + InFocusRadius studs away, blurring toward the camera by NearIntensity and away from it by FarIntensity. The first enabled one in Lighting is used. The player's graphics settings choose how finely it's drawn (UserGameSettings.DepthOfField); it's off at Low."
        }
        ClassName::VignetteEffect => {
            "Inside Lighting: darkens the picture toward its corners, or tints them with its Color. The first enabled one in Lighting is used; off at Low graphics (UserGameSettings.Vignette)."
        }
        ClassName::FilmGrainEffect => {
            "Inside Lighting: moving noise over the picture, as on film, changing every frame. The first enabled one in Lighting is used; off at Low graphics (UserGameSettings.FilmGrain)."
        }
        ClassName::ChromaticAberrationEffect => {
            "Inside Lighting: red and blue drift apart toward the picture's edges, as through a cheap lens. The first enabled one in Lighting is used; off at Low graphics (UserGameSettings.ChromaticAberration)."
        }
        ClassName::SharpenEffect => {
            "Inside Lighting: makes the edges between colors crisper (useful with a render scale below 100%). The first enabled one in Lighting is used; off at Low graphics (UserGameSettings.Sharpen)."
        }
        ClassName::SunRaysEffect => {
            "Inside Lighting: shafts of light streaming from the sun past whatever is in front of it, such as bright sky between leaves or a window, worked out on screen from what's drawn (so they show only while the sun is in or near the view). The first enabled one in Lighting is used; off at Low graphics (UserGameSettings.SunRays)."
        }
        ClassName::MotionBlurEffect => {
            "Inside Lighting: streaks what moves across the view along its motion, as a camera's shutter does: parts that move, posed characters, and everything when the camera moves. What moves with the camera stays sharp. The first enabled one in Lighting is used. The player's graphics settings choose how finely it's drawn (UserGameSettings.MotionBlur); it's off at Low."
        }
        ClassName::OutlineEffect => {
            "Inside Lighting: lines drawn along the edges of everything solid, around each silhouette and along creases where a surface turns sharply, found on screen from what's drawn, so no part needs any setup. A material file's outline draws a line around its own parts instead (see docs/rendering/stylized.md)."
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
        ClassName::Sequence => {
            "Plays a cutscene: a `*.sequence.json` file made in the editor's Sequencer tab, with camera cuts, keyed properties, clips on Animators, Sounds, events, fades, letterbox bars, subtitles from the string table and levels added for a while. Play, Pause and Stop it; EventReached fires as it passes each event and Completed once at its end. With Skippable, the Menu action jumps it to its end; with LockPlayerInput, the player can't move or use the game's input while it plays."
        }
        ClassName::Spline => {
            "A curve through its SplinePoint children, in order (and back to the first when Closed), for SplineFollowers to move parts along, NavigationAgents to patrol and the Sequencer's camera to ride. Measure it with Length, GetPositionAt, GetCFrameAt and GetClosestDistance. Drawn in the editor, never in the game: click it with Select to edit its points."
        }
        ClassName::SplinePoint => {
            "One point of the Spline it's in: where the curve passes (Position), and how it bends there (TangentMode, InTangent and OutTangent)."
        }
        ClassName::Ability => {
            "Something a character can do, in its Model or its Player: a Fireball, a dash, a heal. Activating it (its ActivationKey, or Activate) spends CostAmount of its holder's CostAttribute and starts its Cooldown, unless its holder has one of BlockedByTags; while active it gives GrantsTags. It only activates on the server, which checks again whatever a player's device says. The ability bar shows the player's abilities with their cooldowns."
        }
        ClassName::Effect => {
            "A change to the Model it's in: to an attribute, or a Health or Humanoid property. It's instant, lasts Duration seconds, or repeats every Period (poison, a heal over time); it adds or multiplies, stacks to MaxStacks, and gives GrantsTags while active. Removing it undoes a lasting change. Keep templates out of Models (in ReplicatedStorage, ServerStorage or an Ability) and put copies on with `model:ApplyEffect(effect)`."
        }
        ClassName::SplineFollower => {
            "Moves the Part (with the parts welded to it) or the Model it's parented to along its Spline at Speed during Play: moving platforms, trains and gondolas. It starts at the point of the curve nearest where its parent is, and Loops, PingPongs or stops Once at the end, firing Reached. Characters standing on what it moves ride along."
        }
        ClassName::SplineMesh => {
            "Inside a Spline: bends the model MeshId names along the curve, repeated to fill it with no gaps, for roads, pipes, fences and rails. Its pieces are anchored MeshParts that collide as their bent shapes, made again whenever the spline or its settings change and when the game starts; they're its children, but never saved (Bake in the editor keeps them)."
        }
        ClassName::SplineScatter => {
            "Inside a Spline: places the models Models names along the curve every Spacing studs, or inside a Closed spline at Density, each turned, scaled and moved at random from Seed and stood on the surface below (none where it's steeper than MaxSlope). What it places is made again whenever the spline or its settings change and when the game starts; it's its children, but never saved (Bake in the editor keeps it)."
        }
        ClassName::Decal => {
            "Inside a part: a picture (Texture) on one of its faces (Face), tinted by Color3 and faded by Transparency. It's stretched across the face, or repeated every StudsPerTileU by StudsPerTileV studs. It shows at every graphics quality."
        }
        ClassName::ProjectedDecal => {
            "A box that lays its picture (Texture) onto whatever solid surfaces are inside it, following their shape: bullet holes, puddles, road markings and dirt. The picture faces the box's front (its CFrame's LookVector, -Z), and fades from surfaces turned away from it past half of FadeAngle. With IgnoreCharacters on, characters walking through it stay bare. It isn't drawn at the Low graphics quality, and higher qualities draw more of them, further away. workspace:SpawnDecal places ones that fade out."
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
        ClassName::IntValue => {
            "Holds one whole number in Value (other numbers are rounded). Its `Changed` fires with the new value."
        }
        ClassName::NumberValue => {
            "Holds one number in Value. Its `Changed` fires with the new value."
        }
        ClassName::StringValue => {
            "Holds one string in Value. Its `Changed` fires with the new value."
        }
        ClassName::BoolValue => {
            "Holds one boolean in Value. Its `Changed` fires with the new value."
        }
        ClassName::Vector3Value => {
            "Holds one Vector3 in Value. Its `Changed` fires with the new value."
        }
        ClassName::Color3Value => {
            "Holds one Color3 in Value. Its `Changed` fires with the new value."
        }
        ClassName::CFrameValue => {
            "Holds one CFrame in Value. Its `Changed` fires with the new value."
        }
        ClassName::ObjectValue => {
            "Points at another Instance in Value, or nil. Its `Changed` fires with the new value. `Clone` points a copy at the same place inside the copy; saving keeps the path of names to it."
        }
        ClassName::Camera => {
            "A view of the world. `workspace.CurrentCamera` (LocalScripts only) is the one the player sees through: its `CameraType` says who moves it, and it turns screen points into rays and back. Camera objects placed in the world are named viewpoints, not drawn: `workspace.CurrentCamera.CFrame = workspace.Overlook.CFrame` cuts to one."
        }
        ClassName::Attachment => {
            "A point and a direction on the part it's parented to, for things that need a spot on a part. `Position` and `Orientation` (and `CFrame`) are relative to the part, so it moves with it; `WorldCFrame`, `WorldPosition` and `WorldOrientation` are where it is in the world. `Axis` and `SecondaryAxis` are the ways its X and Y point on the part (`WorldAxis` and `WorldSecondaryAxis` in the world). Not drawn in the game; the editor shows it as a small gizmo."
        }
        ClassName::WeldConstraint => {
            "Holds `Part0` and `Part1` together where they were when it was made (or enabled). Parts welded together move as one body, an assembly: they fall and tumble together, and an anchored part anchors the whole assembly. `part:GetConnectedParts()` and `part:GetRootPart()` follow welds. Setting a welded part's Position or CFrame moves that part alone, and the weld holds it where it lands."
        }
        ClassName::HingeConstraint => {
            "Joins two parts at their attachments so Attachment1's part turns around Attachment0's X axis (its `Axis`), like a door or a wheel. LimitsEnabled keeps it between LowerAngle and UpperAngle; ActuatorType Motor spins it at AngularVelocity, Servo turns it to TargetAngle and holds it. `CurrentAngle` (read-only) is how far it's turned, in degrees."
        }
        ClassName::BallSocketConstraint => {
            "Holds two attachments together and lets them turn any way, like a shoulder or a pendulum. LimitsEnabled keeps Attachment1's axis within UpperAngle of Attachment0's; TwistLimitsEnabled limits twisting around it."
        }
        ClassName::PrismaticConstraint => {
            "Lets Attachment1's part slide along Attachment0's X axis only, like a drawer or a piston. LimitsEnabled keeps it between LowerLimit and UpperLimit; ActuatorType Motor slides it at Velocity, Servo to TargetPosition. `CurrentPosition` (read-only) is how far along it is, in studs."
        }
        ClassName::CylindricalConstraint => {
            "Lets Attachment1's part slide along and turn around Attachment0's X axis, like a drill or a screw. It has a prismatic constraint's limits and actuator for sliding and a hinge's (AngularLimitsEnabled, AngularActuatorType) for turning. `CurrentPosition` and `CurrentAngle` are read-only."
        }
        ClassName::SpringConstraint => {
            "Pulls or pushes its attachments toward FreeLength apart with Stiffness, slowed by Damping, never shorter than MinLength or longer than MaxLength. `CurrentLength` (read-only) is how far apart they are. Visible draws a coil."
        }
        ClassName::RopeConstraint => {
            "Keeps its attachments at most Length apart; closer, it's slack. With WinchEnabled, a winch reels it toward WinchTarget at WinchSpeed, lifting loads up to WinchForce. `CurrentLength` (read-only) is how far apart they are."
        }
        ClassName::RodConstraint => {
            "Keeps its attachments exactly Length apart, letting them turn freely. `CurrentLength` (read-only) is how far apart they are."
        }
        ClassName::LinearVelocity => {
            "Keeps Attachment0's part (and everything welded to it) moving at VectorVelocity, pushing with at most MaxForce through its center of mass. It holds the part up against gravity too, so a zero VectorVelocity holds it still in the air."
        }
        ClassName::AngularVelocity => {
            "Keeps Attachment0's part spinning at AngularVelocity (radians a second), turning it with at most MaxTorque."
        }
        ClassName::VectorForce => {
            "Pushes Attachment0's part with a constant Force every physics step, at Attachment0 or (ApplyAtCenterOfMass) its center of mass. A force of its mass times `workspace.Gravity` upward makes it hover."
        }
        ClassName::Torque => "Turns Attachment0's part with a constant Torque every physics step.",
        ClassName::AlignPosition => {
            "Moves Attachment0's part so Attachment0 reaches Attachment1 (TwoAttachment Mode) or Position (OneAttachment), smoothly, closing most of the gap in about 1/Responsiveness seconds, with at most MaxForce and MaxVelocity. It holds the part up against gravity."
        }
        ClassName::AlignOrientation => {
            "Turns Attachment0's part so Attachment0 faces the way Attachment1 does (TwoAttachment Mode) or CFrame's rotation (OneAttachment), smoothly, in about 1/Responsiveness seconds, with at most MaxTorque and MaxAngularVelocity. A level CFrame keeps a drone level."
        }
        ClassName::Checkpoint => {
            "A place in the world to come back to. `SaveService:SetCheckpoint(checkpoint)` makes it the active one and captures the player data and world state there; `SaveService:RestoreCheckpoint()` puts them back and stands the player character at its Position. A save slot keeps the active checkpoint."
        }
    }
}
