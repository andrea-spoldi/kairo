use gpui::actions;

actions!(
    kairo,
    [
        Quit,
        FocusSearch,
        NavigateUp,
        NavigateDown,
        ConfirmSelection,
        ToggleGrouping,
        OpenCommandPalette,
        CloseCommandPalette,
        OpenSettings,
    ]
);
