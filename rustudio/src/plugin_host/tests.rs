//! Plugin host tests: plugins run against a live edit document, the way the
//! editor drives them (PluginHost::tick once a frame).

use std::sync::Arc;

use rbx_dom_weak::types::Variant;
use rbx_dom_weak::{ustr, InstanceBuilder, WeakDom};

use super::*;

/// Settings go to a per-process temp folder (one for every test module:
/// RUSTUDIO_PLUGIN_DATA is process-wide, so a second folder set by another
/// module's tests would move the files under a running test).
pub(crate) fn temp_data() {
    // (the engine's `__rs_…` internals are hidden in the shipped binary and
    // in a non-test build of the engine crate; plugin tests drive them)
    rustudio_engine::globals_parity::show_internals(true);
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let dir = std::env::temp_dir().join(format!("rustudio-plugin-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::env::set_var("RUSTUDIO_PLUGIN_DATA", &dir);
    });
}

/// A plugin model holding one Script with `src` (plus `extra` items).
fn tree(src: &str, extra: Vec<InstanceBuilder>) -> Arc<WeakDom> {
    let mut dom = WeakDom::new(InstanceBuilder::new("DataModel"));
    let root = dom.root_ref();
    dom.insert(root, InstanceBuilder::new("Script").with_name("Main").with_property("Source", src.to_string()));
    for e in extra {
        dom.insert(root, e);
    }
    Arc::new(dom)
}

fn source(name: &str, src: &str) -> PluginSource {
    PluginSource::Tree { name: format!("user_{name}.rbxmx"), asset_id: None, dom: tree(src, vec![]) }
}

/// Output lines without their " — source" suffix.
fn lines(h: &mut PluginHost) -> Vec<String> {
    h.take_log()
        .into_iter()
        .map(|(_, t)| t.split("  —").next().unwrap_or("").to_string())
        .collect()
}

fn frames(h: &mut PluginHost, doc: &mut Document, n: usize) {
    for _ in 0..n {
        h.tick(doc, 1.0 / 60.0, true, false);
    }
}

fn has(out: &[String], want: &str) -> bool {
    out.iter().any(|l| l == want)
}

#[test]
fn plugin_runs_in_the_edit_document() {
    temp_data();
    let mut doc = Document::new_place();
    let mut h = PluginHost::default();
    h.load(source(
        "Basic",
        r#"
        print("start", typeof(plugin), plugin.ClassName, plugin.Name, script.Name, script.Parent == plugin, plugin.Parent)
        printidentity("ident")
        local RS = game:GetService("RunService")
        print("rs", RS:IsEdit(), RS:IsRunning(), RS:IsStudio(), RS:IsClient(), RS:IsServer(), RS:IsRunMode(), time())
        print("caps", pcall(settings), (pcall(function() return game:GetService("CoreGui") end)))
        print("children", #plugin:GetChildren(), plugin:GetChildren()[2].ClassName)
        local p = Instance.new("Part")
        p.Name = "FromPlugin"
        p.Parent = workspace
        local hb, rs = 0, 0
        RS.Heartbeat:Connect(function() hb += 1 end)
        RS.RenderStepped:Connect(function() rs += 1 end)
        local st = 0
        pcall(function() RS.Stepped:Connect(function() st += 1 end) end)
        task.wait(0.1)
        print("frames", hb >= 5, rs >= 5, st)
        print("loadstring", loadstring("return 1 + 1")())
        "#,
    ))
    .unwrap();
    assert!(!doc.dirty());
    frames(&mut h, &mut doc, 12);
    let out = lines(&mut h);
    assert!(has(&out, "start Instance Plugin user_Basic.rbxmx Main true nil"), "{out:#?}");
    assert!(has(&out, "ident 5"), "{out:#?}");
    assert!(has(&out, "Current asset id is -2"), "{out:#?}");
    assert!(has(&out, "rs true false true true true false 0"), "{out:#?}");
    assert!(has(&out, "caps true true"), "{out:#?}");
    assert!(has(&out, "children 2 MultipleDocumentInterfaceInstance"), "{out:#?}");
    assert!(has(&out, "frames true true 0"), "{out:#?}");
    assert!(has(&out, "loadstring 2"), "{out:#?}");
    // The plugin's Part is in the editor's own document, not a copy.
    let ws = doc.workspace().unwrap();
    assert!(doc.find_named(ws, "FromPlugin").is_some());
    // The plugin's own tree is not in the DataModel and doesn't dirty it…
    let root = doc.dom.root_ref();
    assert!(doc.dom.descendants_of(root).all(|i| i.class != "Plugin"));
    // …but its edit of the place does.
    assert!(doc.dirty());
    assert_eq!(h.loaded()[0].running, true);
}

#[test]
fn own_bookkeeping_keeps_the_place_saved() {
    temp_data();
    let mut doc = Document::new_place();
    let mut h = PluginHost::default();
    h.load(source("Quiet", "local f = Instance.new('Folder') f.Parent = plugin print('ok')")).unwrap();
    frames(&mut h, &mut doc, 3);
    assert!(has(&lines(&mut h), "ok"));
    assert!(!doc.dirty(), "a plugin working on its own instances must not dirty the place");
}

#[test]
fn editor_edits_reach_plugins_as_signals() {
    temp_data();
    let mut doc = Document::new_place();
    let ws = doc.workspace().unwrap();
    let existing = doc.insert(ws, InstanceBuilder::new("Part").with_name("Existing"));
    let mut h = PluginHost::default();
    h.load(source(
        "Watch",
        r#"
        workspace.ChildAdded:Connect(function(c) print("added", c.Name) end)
        workspace.ChildRemoved:Connect(function(c) print("removed", c.Name) end)
        workspace.DescendantAdded:Connect(function(c) print("desc", c.Name) end)
        local e = workspace.Existing
        e.Changed:Connect(function(n) print("changed", n) end)
        e:GetPropertyChangedSignal("Name"):Connect(function() print("renamed to", e.Name) end)
        e.AncestryChanged:Connect(function(_, p) print("ancestry", p and p.Name) end)
        print("ready")
        "#,
    ))
    .unwrap();
    frames(&mut h, &mut doc, 2);
    assert!(has(&lines(&mut h), "ready"));
    // The editor (not Lua) edits the document between frames.
    doc.insert(ws, InstanceBuilder::new("Part").with_name("NewOne"));
    doc.rename(existing, "Renamed");
    doc.set_props("Anchor", vec![(existing, ustr("Anchored"), Variant::Bool(true))], None);
    frames(&mut h, &mut doc, 1);
    let out = lines(&mut h);
    assert!(has(&out, "added NewOne"), "{out:#?}");
    assert!(has(&out, "desc NewOne"), "{out:#?}");
    assert!(has(&out, "changed Name"), "{out:#?}");
    assert!(has(&out, "renamed to Renamed"), "{out:#?}");
    assert!(has(&out, "changed Anchored"), "{out:#?}");
    // Undo reaches them too.
    doc.undo();
    frames(&mut h, &mut doc, 1);
    let out = lines(&mut h);
    assert!(has(&out, "changed Anchored"), "{out:#?}");
    doc.remove(&[existing]);
    frames(&mut h, &mut doc, 1);
    let out = lines(&mut h);
    assert!(has(&out, "removed Renamed"), "{out:#?}");
    assert!(has(&out, "ancestry nil"), "{out:#?}");
    // A plugin's own writes don't come back as edits.
    assert!(!out.iter().any(|l| l.starts_with("added FromPlugin")));
}

#[test]
fn modules_have_no_plugin_global_and_scripts_run_as_studio_picks() {
    temp_data();
    let mut doc = Document::new_place();
    let mut h = PluginHost::default();
    let mut dom = WeakDom::new(InstanceBuilder::new("DataModel"));
    let root = dom.root_ref();
    let main = dom.insert(
        root,
        InstanceBuilder::new("Script")
            .with_name("Main")
            .with_property("Source", "print('main', typeof(require(script.Mod)))".to_string()),
    );
    dom.insert(main, InstanceBuilder::new("ModuleScript").with_name("Mod").with_property("Source", "print('mod plugin', plugin) printidentity('mod') return 1".to_string()));
    dom.insert(main, InstanceBuilder::new("Script").with_name("Nested").with_property("Source", "print('nested', typeof(plugin))".to_string()));
    dom.insert(main, InstanceBuilder::new("LocalScript").with_name("Loc").with_property("Source", "print('local')".to_string()));
    dom.insert(main, InstanceBuilder::new("Script").with_name("Off").with_property("Source", "print('off')".to_string()).with_property("Disabled", true));
    let rc = |v: u32| Variant::Enum(rbx_dom_weak::types::Enum::from_u32(v));
    dom.insert(main, InstanceBuilder::new("Script").with_name("Srv").with_property("Source", "print('server ctx')".to_string()).with_property("RunContext", rc(1)));
    dom.insert(main, InstanceBuilder::new("Script").with_name("Cli").with_property("Source", "print('client ctx')".to_string()).with_property("RunContext", rc(2)));
    dom.insert(main, InstanceBuilder::new("Script").with_name("Plg").with_property("Source", "print('plugin ctx')".to_string()).with_property("RunContext", rc(3)));
    h.load(PluginSource::Tree { name: "user_Tree.rbxmx".into(), asset_id: None, dom: Arc::new(dom) }).unwrap();
    frames(&mut h, &mut doc, 2);
    let out = lines(&mut h);
    let order: Vec<String> = out.iter().filter(|l| !l.starts_with("Current asset")).cloned().collect();
    assert_eq!(order, ["mod plugin nil", "mod 5", "main number", "nested Instance", "local", "plugin ctx"], "{out:#?}");
}

#[test]
fn errors_are_attributed_and_reported() {
    temp_data();
    let mut doc = Document::new_place();
    let mut h = PluginHost::default();
    h.load(source("Err", "local x = nil\nprint(x.y)")).unwrap();
    frames(&mut h, &mut doc, 1);
    let out = lines(&mut h);
    assert!(has(&out, "user_Err.rbxmx.Main:2: attempt to index nil with 'y'"), "{out:#?}");
    assert!(out.iter().any(|l| l.ends_with("Script 'user_Err.rbxmx.Main', Line 2")), "{out:#?}");
    let info = &h.loaded()[0];
    assert_eq!(info.errors, vec!["user_Err.rbxmx.Main:2: attempt to index nil with 'y'".to_string()]);
    assert!(h.result_json()[0]["errors"][0].as_str().unwrap().contains("attempt to index nil"));
}

#[test]
fn unload_fires_unloading_then_stops_everything() {
    temp_data();
    let mut doc = Document::new_place();
    let mut h = PluginHost::default();
    let id = h
        .load(source(
            "Bye",
            r#"
            local n = 0
            game:GetService("RunService").Heartbeat:Connect(function() n += 1 end)
            task.spawn(function() while true do task.wait(0.05) _G.bye_ticks = n end end)
            plugin.Unloading:Connect(function() print("unloading", plugin.Parent, script.Parent == plugin) end)
            plugin.Destroying:Connect(function() print("destroying") end)
            "#,
        ))
        .unwrap();
    frames(&mut h, &mut doc, 10);
    h.unload(id);
    frames(&mut h, &mut doc, 1);
    let out = lines(&mut h);
    assert!(has(&out, "unloading nil true"), "{out:#?}");
    assert!(!has(&out, "destroying"), "Studio fires no Destroying on unload: {out:#?}");
    assert!(h.loaded().is_empty());
}

/// A thread parked inside a required ModuleScript's function (Time +'s
/// PluginManager `repeat task.wait(1)`, AutoScale's StudioLoaded wait) ends
/// with the plugin: it never resumes into the unloaded Plugin.
#[test]
fn unload_stops_threads_waiting_inside_modules() {
    temp_data();
    let mut doc = Document::new_place();
    let mut h = PluginHost::default();
    let module = InstanceBuilder::new("ModuleScript").with_name("Waiter").with_property(
        "Source",
        r#"
        local m = {}
        function m.later(p) task.wait(0.2) print("module resumed") p:CreateToolbar("after") end
        function m.signal(p) game:GetService("RunService").Heartbeat:Wait() task.wait(0.2) print("signal resumed") end
        return m
        "#
        .to_string(),
    );
    let dom = tree(
        r#"
        local W = require(script.Parent.Waiter)
        task.spawn(W.signal, plugin)
        W.later(plugin)
        print("main resumed")
        "#,
        vec![module],
    );
    let id = h.load(PluginSource::Tree { name: "user_ModWait.rbxmx".into(), asset_id: None, dom }).unwrap();
    frames(&mut h, &mut doc, 3);
    h.unload(id);
    frames(&mut h, &mut doc, 40);
    let out = lines(&mut h);
    assert!(!out.iter().any(|l| l.contains("resumed") || l.contains("not loaded")), "{out:#?}");
    assert!(h.loaded().is_empty());
}

#[test]
fn settings_round_trip_and_share_between_local_plugins() {
    temp_data();
    let mut doc = Document::new_place();
    let mut h = PluginHost::default();
    h.load(source(
        "SetA",
        r#"
        plugin:SetSetting("st_n", 5)
        plugin:SetSetting("st_t", {a = 1, b = {true, "x"}})
        plugin:SetSetting("st_v", Vector3.new(1, 2, 3))
        plugin:SetSetting("st_e", Enum.Material.Grass)
        plugin:SetSetting("st_i", workspace)
        plugin:SetSetting("st_m", {1, 2, x = 3})
        plugin:SetSetting("st.dot", 7)
        plugin:SetSetting("st_nan", 0/0)
        print("getA", plugin:GetSetting("st_n"), plugin:GetSetting("st_t").b[2], plugin:GetSetting("st_v"),
            plugin:GetSetting("st_e"), plugin:GetSetting("st_i"), #plugin:GetSetting("st_m"),
            plugin:GetSetting("st_dot"), plugin:GetSetting("st_nan"), plugin:GetSetting("missing"))
        print("argerr", pcall(function() plugin:GetSetting() end))
        "#,
    ))
    .unwrap();
    frames(&mut h, &mut doc, 1);
    h.load(source("GetB", "print('getB', plugin:GetSetting('st_n'))")).unwrap();
    frames(&mut h, &mut doc, 1);
    let out = lines(&mut h);
    assert!(has(&out, "getA 5 x 1, 2, 3 Grass Instance 2 7 nil nil"), "{out:#?}");
    assert!(has(&out, "argerr false Argument 1 missing or nil"), "{out:#?}");
    assert!(has(&out, "getB 5"), "{out:#?}");
    let file = settings::file("0");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.starts_with("{ ") && text.contains("\"st_v\":\"1, 2, 3\""), "{text}");
}

#[test]
fn toolbar_button_and_action_fire_from_the_harness() {
    temp_data();
    let mut doc = Document::new_place();
    let mut h = PluginHost::default();
    h.load(source(
        "Ui",
        r#"
        local tb = plugin:CreateToolbar("Tools")
        local b = tb:CreateButton("Go", "tip", "", "Go!")
        print("objs", tb.ClassName, tb.Name, tb.Parent, b.ClassName, b.Enabled)
        b.Click:Connect(function() print("click") end)
        local a = plugin:CreatePluginAction("act", "Act", "tip")
        print("action", a.ActionId, a.Name)
        a.Triggered:Connect(function() print("triggered") end)
        print("argerr", pcall(function() plugin:CreatePluginAction() end))
        print("dup", pcall(function() plugin:CreatePluginAction("act", "Act", "tip") end))
        local m = plugin:GetMouse()
        print("mouse", m.ClassName, m.Name, m.X, m.Y, m == plugin:GetMouse())
        local info = DockWidgetPluginGuiInfo.new(Enum.InitialDockState.Float, true, false, 300, 200, 100, 80)
        print("info", tostring(info), info.FloatingXSize, pcall(function() return info.InitialDockState end))
        local w = plugin:CreateDockWidgetPluginGui("W", info)
        print("widget", w.ClassName, w.Name, w.Title, w.Enabled, w.Parent and w.Parent.Name)
        "#,
    ))
    .unwrap();
    frames(&mut h, &mut doc, 1);
    h.trigger(Trigger::parse("plugin-button:Tools/Go").unwrap());
    h.trigger(Trigger::parse("plugin-action:act").unwrap());
    frames(&mut h, &mut doc, 1);
    let out = lines(&mut h);
    assert!(has(&out, "objs PluginToolbar Instance nil PluginToolbarButton true"), "{out:#?}");
    assert!(has(&out, "action user_Ui.rbxmx_act Instance"), "{out:#?}");
    assert!(has(&out, "argerr false Argument 3 missing or nil"), "{out:#?}");
    assert!(has(&out, "dup false This plugin cannot create more than one PluginAction with id: \"act\""), "{out:#?}");
    assert!(has(&out, "mouse PluginMouse Instance -1 -1 true"), "{out:#?}");
    assert!(
        has(&out, "info InitialDockState:Float InitialEnabled:1 InitialEnabledShouldOverrideRestore:0 FloatingXSize:300 FloatingYSize:200 MinWidth:100 MinHeight:80 300 false 'InitialDockState' is not a valid member of DockWidgetPluginGuiInfo"),
        "{out:#?}"
    );
    assert!(has(&out, "widget DockWidgetPluginGui PluginGui  true PluginGuiService"), "{out:#?}");
    assert!(has(&out, "click"), "{out:#?}");
    assert!(has(&out, "triggered"), "{out:#?}");
    let kinds: Vec<_> = h.ui_items().into_iter().map(|u| u.kind).collect();
    assert!(kinds.iter().any(|k| matches!(k, UiKind::Button { id, .. } if id == "Go")));
    assert!(kinds.iter().any(|k| matches!(k, UiKind::DockWidget { id, info } if id == "W" && info.initial_dock_state == "Float")));
    // The widget's service isn't saved with the place.
    let bytes = doc.encode_all(true).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("PluginGuiService"));
}

#[test]
fn selection_changes_announce_after_the_caller_yields() {
    temp_data();
    let mut doc = Document::new_place();
    let ws = doc.workspace().unwrap();
    let a = doc.insert(ws, InstanceBuilder::new("Part").with_name("A"));
    let mut h = PluginHost::default();
    h.load(source(
        "Sel",
        r#"
        local S = game:GetService("Selection")
        local n = 0
        S.SelectionChanged:Connect(function() n += 1 print("changed", #S:Get()) end)
        S:Set({workspace.A})
        S:Add({workspace.A, workspace})
        print("right after", n, #S:Get())
        print("err1", pcall(function() S:Set({1}) end))
        print("err2", pcall(function() S:Set(workspace) end))
        "#,
    ))
    .unwrap();
    frames(&mut h, &mut doc, 1);
    let out = lines(&mut h);
    assert!(has(&out, "right after 0 2"), "{out:#?}");
    assert_eq!(out.iter().filter(|l| l.starts_with("changed")).count(), 2, "{out:#?}");
    assert!(has(&out, "err1 false Unable to cast value to Object"), "{out:#?}");
    assert!(has(&out, "err2 false Expected table argument, got userdata"), "{out:#?}");
    assert_eq!(doc.selection.len(), 2);
    // The editor selects: one SelectionChanged next frame.
    doc.selection = vec![a];
    frames(&mut h, &mut doc, 2);
    let out = lines(&mut h);
    assert_eq!(out, vec!["changed 1".to_string()], "{out:#?}");
}

#[test]
fn activation_is_one_plugin_at_a_time() {
    temp_data();
    let mut doc = Document::new_place();
    let mut h = PluginHost::default();
    h.load(source("Act1", "plugin.Deactivation:Connect(function() print('act1 deactivated', plugin:IsActivated()) end) plugin:Activate(true) _G.act1 = plugin")).unwrap();
    frames(&mut h, &mut doc, 1);
    assert!(h.active_plugin().is_some());
    assert!(h.take_activation_change());
    h.load(source("Act2", "plugin:Activate(false) print('act2', plugin:IsActivated(), _G.act1:IsActivated()) plugin:Deactivate() plugin:Deactivate() print('act2 off', plugin:IsActivated())")).unwrap();
    frames(&mut h, &mut doc, 1);
    let out = lines(&mut h);
    assert!(has(&out, "act1 deactivated false"), "{out:#?}");
    assert!(has(&out, "act2 true false"), "{out:#?}");
    assert!(has(&out, "act2 off false"), "{out:#?}");
    assert!(h.active_plugin().is_none());
}

#[test]
fn new_document_reloads_plugins() {
    temp_data();
    let mut doc = Document::new_place();
    let mut h = PluginHost::default();
    h.load(source("Again", "print('loaded in', game.Name ~= nil)")).unwrap();
    frames(&mut h, &mut doc, 1);
    assert_eq!(lines(&mut h).iter().filter(|l| l.starts_with("loaded in")).count(), 1);
    let mut other = Document::new_place();
    frames(&mut h, &mut other, 1);
    assert_eq!(lines(&mut h).iter().filter(|l| l.starts_with("loaded in")).count(), 1);
}

#[test]
fn play_copies_run_with_play_values() {
    temp_data();
    let mut doc = Document::new_place();
    let mut h = PluginHost::default();
    h.load(source("Both", "local RS = game:GetService('RunService') print('dm', RS:IsEdit(), RS:IsRunning(), RS:IsServer(), RS:IsClient()) printidentity('id')")).unwrap();
    frames(&mut h, &mut doc, 1);
    let out = lines(&mut h);
    assert!(has(&out, "dm true false true true"), "{out:#?}");
    // A Play server: the same plugin loads again there.
    let rt = crate::studio_hooks::new_runtime().unwrap();
    rt.enter(World::new(doc.clone_for_play("p".into()), RunMode::Play));
    rt.bind_globals().unwrap();
    h.load_into_play(&rt);
    rt.dispatch();
    h.unload_from_play(&rt);
    let w = rt.leave();
    let out: Vec<String> = w.output.iter().map(|(_, t)| t.split("  —").next().unwrap_or("").to_string()).collect();
    assert!(has(&out, "dm false true true false"), "{out:#?}");
    assert!(has(&out, "id 5"), "{out:#?}");
}

#[test]
fn plugins_folder_order() {
    let dir = std::env::temp_dir().join(format!("rustudio-plugin-order-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("Sub/Deeper")).unwrap();
    for f in ["Zz.lua", "Aa.lua", "b.LUA", "Ab.rbxmx", "Zy.rbxmx", "M.rbxm", "skip.luau", "x.txt", "Sub/In.lua", "Sub/Deeper/D.lua", "Sub/no.rbxmx"] {
        std::fs::write(dir.join(f), "").unwrap();
    }
    let names: Vec<String> = source::dir_order(&dir).unwrap().iter().map(|s| s.plugin_name()).collect();
    assert_eq!(
        names,
        ["user_Aa.lua", "user_b.LUA", "user_Sub/Deeper/D.lua", "user_Sub/In.lua", "user_Zz.lua", "user_Ab.rbxmx", "user_Zy.rbxmx", "user_M.rbxm"]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn turned_off_loads_nothing() {
    let mut h = PluginHost::default();
    h.set_enabled(false);
    assert!(h.load(source("No", "print('no')")).is_err());
    let mut doc = Document::new_place();
    frames(&mut h, &mut doc, 1);
    assert!(lines(&mut h).iter().all(|l| l != "no"));
}

#[test]
fn studio_theme_colors_as_studio() {
    temp_data();
    let mut doc = Document::new_place();
    let mut h = PluginHost::default();
    h.load(source(
        "Theme",
        r#"
        local th = settings().Studio.Theme
        print("theme", th.ClassName, th.Name)
        print("bg", th:GetColor(Enum.StudioStyleGuideColor.MainBackground):ToHex(), th:GetColor("MainBackground"):ToHex())
        print("sb", th:GetColor(Enum.StudioStyleGuideColor.ScrollBar, Enum.StudioStyleGuideModifier.Pressed):ToHex())
        print("err", pcall(function() return th:GetColor() end))
        "#,
    ))
    .unwrap();
    frames(&mut h, &mut doc, 1);
    let out = lines(&mut h);
    // Studio 0.747's Dark palette (studio_theme.txt).
    assert!(has(&out, "theme StudioTheme Dark"), "{out:#?}");
    assert!(has(&out, "bg 191a1f 191a1f"), "{out:#?}");
    assert!(has(&out, "sb 494d5a"), "{out:#?}");
    assert!(has(&out, "err false Argument 1 missing or nil"), "{out:#?}");
}

#[test]
fn input_object_changed_and_property_signals_fire_once_per_write() {
    temp_data();
    let mut doc = Document::new_place();
    let mut h = PluginHost::default();
    h.load(source(
        "IoSignals",
        r#"
        local io = __rs_new_input_object({ UserInputType = Enum.UserInputType.MouseButton1 })
        local n, c = 0, {}
        io:GetPropertyChangedSignal("UserInputState"):Connect(function() n += 1 end)
        io.Changed:Connect(function(p) table.insert(c, p) end)
        io.UserInputState = Enum.UserInputState.End
        io.UserInputState = Enum.UserInputState.End -- same value: no signal
        io.Position = Vector3.new(1, 2, 0)
        local ok, err = pcall(function() return io:GetPropertyChangedSignal("Nope") end)
        print("io", n, table.concat(c, ","), ok, err)
        "#,
    ))
    .unwrap();
    frames(&mut h, &mut doc, 2);
    let out = lines(&mut h);
    assert!(has(&out, "io 1 UserInputState,Position false user_IoSignals.rbxmx.Main:9: Nope is not a valid property name."), "{out:#?}");
}

#[test]
fn command_identity_gets_plugin_manager_interface() {
    use rustudio_engine::runtime::security::{with_main_identity, COMMAND};

    let rt = crate::studio_hooks::new_runtime().unwrap();
    rt.enter(World::new(Document::new_place(), RunMode::Run));
    rt.bind_globals().unwrap();
    let class = with_main_identity(&rt.lua, COMMAND, || {
        rt.lua.load("return PluginManager().ClassName").eval::<String>()
    }).unwrap();
    assert_eq!(class, "PluginManagerInterface");
}
