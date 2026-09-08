#![cfg(feature = "accessibility")]

use baseview::accesskit;

#[test]
fn test_accesskit_event_conversion_and_tree_update() {
    // 1. Test accessibility event conversion into egui input (`egui::Event::AccessKitActionRequest`).
    let action_request = accesskit::ActionRequest {
        action: accesskit::Action::Focus,
        target_tree: accesskit::TreeId(Default::default()),
        target_node: accesskit::NodeId(1),
        data: None,
    };

    let baseview_event = baseview::Event::Accessibility(baseview::AccessibilityEvent::ActionRequested(
        action_request.clone(),
    ));

    let mut egui_input = egui::RawInput::default();
    if let baseview::Event::Accessibility(baseview::AccessibilityEvent::ActionRequested(req)) =
        baseview_event
    {
        egui_input
            .events
            .push(egui::Event::AccessKitActionRequest(req));
    }

    assert_eq!(egui_input.events.len(), 1);
    match &egui_input.events[0] {
        egui::Event::AccessKitActionRequest(req) => {
            assert_eq!(req.action, accesskit::Action::Focus);
            assert_eq!(req.target_node, accesskit::NodeId(1));
        }
        _ => panic!("Expected AccessKitActionRequest event"),
    }

    // 2. Run an egui pass (begin_pass / end_pass) with an interactive widget (e.g. egui::Button).
    let ctx = egui::Context::default();
    ctx.enable_accesskit();

    ctx.begin_pass(egui_input);
    let mut ui = egui::Ui::new(
        ctx.clone(),
        egui::Id::new("test_ui"),
        egui::UiBuilder::default(),
    );
    egui::CentralPanel::default().show(&mut ui, |ui| {
        let _ = ui.button("Click me");
    });
    let mut full_output = ctx.end_pass();
    full_output.textures_delta.clear();

    // 3. Verify `full_output.platform_output.accesskit_update` produces an `accesskit::TreeUpdate`.
    let accesskit_update = full_output
        .platform_output
        .accesskit_update
        .expect("Expected accesskit_update to be generated when AccessKit is enabled");

    // 4. Verify that this `TreeUpdate` type matches `baseview::accesskit::TreeUpdate`
    // and `baseview::WindowContext::update_accessibility_tree` signature.
    fn assert_tree_update_compatibility(update: baseview::accesskit::TreeUpdate) {
        fn _check_signature(_ctx: &mut baseview::WindowContext, _update: baseview::accesskit::TreeUpdate) {
            #[allow(unreachable_code)]
            {
                _ctx.update_accessibility_tree(_update);
            }
        }
        assert!(!update.nodes.is_empty(), "TreeUpdate should contain nodes");
    }

    assert_tree_update_compatibility(accesskit_update);
}
