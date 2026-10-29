# Data model

How Instances are stored and looked up.

- One tree of **Instances**: class, `Name`, `Parent`, children, typed properties and events.
- Instances live in an **arena** and are referenced by **generational IDs** (index plus generation). Never use `Rc<RefCell<…>>` parent/child pointers.
- **Network IDs** (Phase 64): a replica world (`World::empty_replica`) holds its server's Instances at their own IDs (`World::create_at`, `Arena::insert_at`) and gives its own Instances IDs from 2³¹ up, which the server never uses (`InstanceId::is_local`). So `InstanceId::to_bits` is the same number for the same Instance on every device. ServerScriptService and ServerStorage are `World::server_only_services`.
- An ID can outlive its Instance. Every lookup returns an `Option` or `Result`; never assume the Instance still exists.
- An Instance can be the top of a placed **prefab copy** (Phase 49): `Instance::prefab` is its `PrefabLink` (`World::set_prefab`, not a `Change`). `core::prefab` matches a copy to what it was made from, works out its overrides and re-makes it from a new source (`sync`); see `docs/project/world.md`.
- Besides its properties, an Instance has **attributes** (name to `Value`, sorted) and **tags** (in the order added). Change them through `World::set_attribute`, `add_tag` and `remove_tag`, which record `Change::Attribute` and `Change::Tag`. `World::tagged` and `all_tags` only count Instances inside `game`.
- A property can point at another Instance (`Value::Instance`, `ObjectValue.Value`). It holds an `InstanceId`, so it can go stale like any other; `World::duplicate` points a copy's references into the copy when the target was copied too.
