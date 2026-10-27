# Data model

How Instances are stored and looked up.

- One tree of **Instances**: class, `Name`, `Parent`, children, typed properties and events.
- Instances live in an **arena** and are referenced by **generational IDs** (index plus generation). Never use `Rc<RefCell<…>>` parent/child pointers.
- An ID can outlive its Instance. Every lookup returns an `Option` or `Result`; never assume the Instance still exists.
- Besides its properties, an Instance has **attributes** (name to `Value`, sorted) and **tags** (in the order added). Change them through `World::set_attribute`, `add_tag` and `remove_tag`, which record `Change::Attribute` and `Change::Tag`. `World::tagged` and `all_tags` only count Instances inside `game`.
- A property can point at another Instance (`Value::Instance`, `ObjectValue.Value`). It holds an `InstanceId`, so it can go stale like any other; `World::duplicate` points a copy's references into the copy when the target was copied too.
