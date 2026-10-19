use gpui::actions;

actions!(
    litemark,
    [
        ToggleBold,
        ToggleItalic,
        ToggleUnderline,
        ToggleStrikethrough,
        SaveFile,
        OpenFile,
        NewTab,
        CloseActiveTab,
        FindReplace,
        ToggleSidebar,
        ToggleZenMode,
        ExitZenMode,
        ZoomIn,
        ZoomOut,
        ResetZoom,
        ToggleViewMode,
        UndoAction,
        RedoAction,
        IndentAction,
        OutdentAction,
        CloseModal,
    ]
);
