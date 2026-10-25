# M18 data check (2026-10-25)

Checks Milestone 18's numbers against Boundless's own data files, reachable from the sandbox this time. Facts and numbers only; internal item names below are identifiers for lookup, never names to use in game. Confidence is **high** for anything read straight from a data file unless marked.

## Sources (fetched 2026-10-25, decoded in the scratchpad, not committed)

| Tag | File | What it holds |
|---|---|---|
| **[MAY-R]** | `boundless.mayumi.fi/recipes/raw/recipes.msgpack` (the game's own recipe file; decoded with the site's `mapMsgpackKeys`) | per recipe: `outputItem`, `outputQuantity[3]`, `inputs[].inputQuantity[3]`, `machine`, `machineLevel`, `duration[3]`, `powerRequired`, `spark[3]`, `heat`, `wear[3]`, `craftXP`, `knowledgeUnlockLevel`, `recipeGroupName`. `[3]` = single / bulk / mass. 826 recipes, 19 input groups |
| **[MAY-I]** | `boundless.mayumi.fi/recipes/raw/compileditems.msgpack` | the game's compiled item table (2,283 items): `fuelData`, `repairPointsOnUse`, `chiselType`, `chiselPrecision`, `attributeModifierBundles` (names only, no values), `categories` |
| **[BI-R]** | `boundlessinfo.com/assets/data/compiled-recipes.json` | the same recipes (870; a later patch adds melee weapons and shields) plus `skillsNeeded`. Every field shared with [MAY-R] is identical on all 826 common recipes (checked by script) |
| **[BI-A]** | `boundlessinfo.com/assets/data/item-attributes.json` | resolved attribute values per gear item (tools, spanners, chisels, grapples, ...) |
| **[PF-D]** | `pfiffel.com/bl/drops/dataBlockDrops.js` `aBlocks[id].drops` | per block: `[item, chance, tier]` (tier 0-based, −1 = every tier); expected count = sum of entries for that tier |
| **[PF-S]** | `pfiffel.com/bl/blocks/dataBlockStats.js` `aBlocks[].mineXP / buildXP` | mining and building XP per block |
| **[PF-C]** | `pfiffel.com/bl/dmg/damageCalculatorCreatures.js` `aWorldMods`, `damageCalculator.js` | creature modifiers for elemental and special worlds |
| **[MAY-F]** | `boundless.mayumi.fi/forge/raw/forge.json` `data.*` | forge momentum constants |
| **[BLX]** | `api.boundlexx.app/api/v1/skills/` | the game's skill text (the API's `/items/`, `/blocks/`, `/exports/` still return 500) |

Boundmore name ↔ data name used below: fossil essence = `ESSENCE_VITAL_ANCIENT`; vital essence = `ESSENCE_VITAL_FRESH`; strong bonding agent = `ITEM_BONDINGAGENT_ENRICHED`; relic fragment / component / core = `ITEM_ANCIENTTECHNOLOGY_FRAGMENT / _COMPONENT / _DEVICE`; coil = `MACHINE_ENGINE_*`; titanium furnace base = `MACHINE_FIREPIT_TITANIUM`; medium coal = `ITEM_COAL_BASE` / `COAL_DEFAULT_*`. Forge: tincture = `FORGE_COMPOUND`, glaze = `FORGE_GUM`, salve = `FORGE_PASTE`, spur = `FORGE_CATALYST`, wash = `FORGE_SOLVENT`, wax = `FORGE_RESIN`.

## Headline: what differs from M18's spec and data

| # | Topic | M18 (spec / assets) | Data | Source |
|---|---|---|---|---|
| 1 | **Atmosphere food is per line** | one food line protects against all three (rule 3, **guess**) | 9 foods: chilli, biryani, pie × Caustic, Potent, Volatile. Each is made with an amalgam of its line (Caustic = corrosion or toxin amalgam; Potent = shock or chill; Volatile = blast or burn) and carries its own buff bundle (`Item Caustic Protecting Chilli Initial`, ...) | [MAY-R] groups `Any Caustic/Potent/Volatile Amalgam`; [MAY-I] `attributeModifierBundles` |
| 2 | Atmosphere food recipes | guessed inputs (prime meat, spicy beans, water, rice, pie) | see "Atmosphere food" below: needs amalgams (elemental shard + fresh blood, compactor), so needs elemental creatures' shards | [MAY-R] |
| 3 | Atmosphere food skill gates | Food Crafting 2 / 3 / 4 | 1 / 3 / 5 | [BI-R] `skillsNeeded` |
| 4 | Purified buckets' power | 1,500 (rule 36) | **0** (Powered level, 100 spark); only **fortified** buckets need 1,500 | [MAY-R] `powerRequired` |
| 5 | Coil inputs | any of machined copper / iron / gold alloy (basic); any of titanium / gold / silver (advanced) | one fixed metal per coil: basic workbench, extractor, mixer = machined copper; compactor, refinery = machined iron; forge = machined gold. Advanced workbench, extractor, mixer = machined silver; compactor, refinery = machined gold; forge = machined titanium | [MAY-R] `inputs` |
| 6 | Topaz and sapphire grapples | 1 relic fragment, no component | **1 component + 1 fragment** (other gems 1 + 5) | [MAY-R] |
| 7 | Titanium gear skill | Tool / Tech Crafting 3 | **2** (gem gear 3 is right) | [BI-R] |
| 8 | Craft XP and machine wear (all M18 recipes) | guesses on the silver line (e.g. titanium hammer 40 XP / 22 wear) | real values in every table below, e.g. titanium hammer 100 XP / 70 wear, gem tool 160 / 100, coil 90 / 55, advanced coil 180 / 100. Boundmore's copper and iron rows already equal the data, so the data's scale is Boundmore's | [MAY-R] `craftXP`, `wear[0]` |
| 9 | Spanners | silver 1200 / 30 / 13, gold 1100 / 30 / 14, titanium 1500 / 30 / 20 (dur / speed / energy, **guess**); gem energy 30 | silver **1100 / 70 / 10**, gold **1000 / 40 / 15**, titanium **1500 / 50 / 30**, gems **2000 / 50 / 50** | [BI-A] Spanners |
| 10 | Mining XP | gems 120 (**guess**), titanium 100, hard coal 60, medium coal 36, silver 60, relic core 80, relic cache 40 | gems **176**, titanium **136**, hard coal **104**, medium coal **64**, silver **64**, ancient tech device **104**, component **64**; gold 80 (matches) | [PF-S] `mineXP` |
| 11 | Gem chisel cuts | ruby slope plain, topaz slope precise | **ruby precise**, **topaz plain** (type 0 = the copper and silver chisels' cut, read as slope) | [MAY-I] `chiselType`, `chiselPrecision` |
| 12 | Chisel Flexibility | gem chisels take their gem's Flex (70 to 80) | every chisel Flex **100**; basic chisel rank 50, advanced rank 100 | [BI-A] Chisels |
| 13 | Large fossils | "wait for power" | large fossil seam: 1.0 / 1.5 T5 / 1.8 T6-7 / 2.2 T8, mine XP 104; 1 large fossil → **10** fossil essence, extractor, 600 s, 200 spark, **500 power** | [PF-D], [MAY-R] |
| 14 | Bark | trunks only | trunks (3% T3-5, 4% T6+) **and** an extractor recipe: 2 nuts → 1 bark, 120 s, 100 spark, 500 power | [PF-D], [MAY-R] |
| 15 | Forge ingredient power | "tier 2 and 3 tinctures, most salves, spurs, washes, unmaking wax 2 and 3 need power" | confirmed, with exact values per ingredient (table below); boon wash 1 and the flaw and quirk shifting washes need **no** power (Powered: spark only) | [MAY-R] |
| 16 | Furnace heat of coal | medium 2.5×, hard 8× soft (**guess**) | fuel has one value for spark and heat (`fuelData.sparkAmount`): soft 100, medium 250, hard 800, so the ratios hold | [MAY-I] `fuelData` |
| 17 | Elemental creatures | +100% health, +50% damage and armour | same, plus **double drops and +50% XP**; "special" (Lucent) worlds +200% health, +100% damage and armour, triple drops, +75% XP | [PF-C] `aWorldMods` |

Confirmed unchanged (data = M18): all times, spark, heat and power on smelting, alloys, compact/refined/machined chains, gems, coils, gem gear (1,500), compact coal (500 / 1,000 / 2,000), leather (500), titanium furnace; tool, grapple and gem tool stats (tuning 2.1 and 4.9 exactly); drop tables of every M18 seam and trunk bark; spanner repair per hit.

## 1. Machine level and power

`machineLevel` is a label that follows the recipe's needs, not a separate threshold ([MAY-R], all 826 recipes): **Standard** = no spark, no power (301); **Powered** = spark, power 0 (198); **Overdriven** = power 200 to 2,000 (271); **Supercharged** = power 2,000 to 6,800 (55). Nothing in the recipe or item files gives the speed-up formula.

### Every recipe with nonzero power ([MAY-R] `powerRequired`)

| Power | Level | Machine | Outputs (internal names) |
|---|---|---|---|
| 200 | Overdriven | COMPACTOR | CROPDROP_FUEL_COMPACT |
| 200 | Overdriven | MIXER | COAL_SOFT_ENRICHED, SEASONAL_BIRTHDAY_DOLL |
| 400 | Overdriven | COMPACTOR | METAL_COPPER_MACHINED_GRID, METAL_IRON_MACHINED_GRID |
| 400 | Overdriven | REFINERY | METAL_COPPER_DECORATIVE_FILIGREE, METAL_COPPER_MACHINED_CIRCUITBOARD, METAL_IRON_DECORATIVE_FILIGREE, METAL_IRON_MACHINED_CIRCUITBOARD |
| 500 | Overdriven | COMPACTOR | AUGMENT_BOMB_AREAOFEFFECT2, AUGMENT_BOMB_CRITICALEFFECT2, AUGMENT_BOMB_DAMAGE2, AUGMENT_BOMB_PROJECTILESPEED2, AUGMENT_SLINGBOW_ACTIONSPEED2, AUGMENT_SLINGBOW_CRITICALEFFECT2, AUGMENT_SLINGBOW_DAMAGE2, AUGMENT_SLINGBOW_MAXLENGTH2, AUGMENT_SLINGBOW_PROJECTILESPEED2, COAL_SOFT_COMPACT |
| 500 | Overdriven | EXTRACTOR | ESSENCE_VITAL_ANCIENT, FORGE_COMPOUND_BOON_IMPERFECT2, FORGE_COMPOUND_BOON_PURE2, FORGE_COMPOUND_BOON_UNSTABLE2, FORGE_COMPOUND_REDUCTION_DEFECT1, FORGE_COMPOUND_REDUCTION_DEFECT_UNSTABLE1, FORGE_COMPOUND_REDUCTION_QUIRK1, FORGE_COMPOUND_REDUCTION_QUIRK_IMPERFECT1, ITEM_BARK_BASE |
| 500 | Overdriven | MIXER | BREW_CLEANSING_GREATER, BREW_FAST_GREATER, BREW_HEALING_GREATER, BREW_HEALING_GREATER_INSTANT, BREW_INNOCUOUS_GREATER, BREW_LUMINOUS_GREATER, BREW_RECOVERY_GREATER, BREW_RECOVERY_GREATER_INSTANT, BREW_STRENGTH_GREATER, ITEM_BREW_FOCUS, ITEM_BREW_RAGE, ITEM_OIL_CHANELLING |
| 500 | Overdriven | REFINERY | ITEM_LEATHER_BASE, SIGN_METAL_BASE, SIGN_METAL_MODULAR, STORAGE_FARMED_STYLISH, STORAGE_STONE_STYLISH, STORAGE_WOOD_STYLISH |
| 500 | Overdriven | WORKBENCH | ITEM_BREW_CONTAINER3, TOOL_FERTILIZER_INORGANIC |
| 600 | Overdriven | EXTRACTOR | FORGE_PASTE_BOOST_BOON1, FORGE_PASTE_ENVIGORATION1, FORGE_PASTE_FATE1, FORGE_PASTE_GUARD_DEFECT1, FORGE_PASTE_GUARD_QUIRK1, FORGE_PASTE_PROTECTION1, FORGE_PASTE_STABILISATION1 |
| 600 | Overdriven | MIXER | BREW_SMOOTHIE_HEALING_GREATER, COAL_DEFAULT_ENRICHED |
| 800 | Overdriven | COMPACTOR | ICE_DEFAULT_DECORATIVE, METAL_GOLD_MACHINED_GRID, METAL_SILVER_MACHINED_GRID |
| 800 | Overdriven | MIXER | SAND_CONCRETE_REFINED |
| 800 | Overdriven | REFINERY | METAL_GOLD_DECORATIVE_FILIGREE, METAL_GOLD_MACHINED_CIRCUITBOARD, METAL_SILVER_DECORATIVE_FILIGREE, METAL_SILVER_MACHINED_CIRCUITBOARD, ROCK_MARBLE_REFINED |
| 1000 | Overdriven | COMPACTOR | COAL_DEFAULT_COMPACT |
| 1000 | Overdriven | EXTRACTOR | ESSENCE_VITAL_FRESH, FORGE_COMPOUND_BOON_CORRUPTED1, FORGE_COMPOUND_BOON_DRAINING1, FORGE_PASTE_GAMBLING1, FORGE_RESIN_DECONSTRUCTION2 |
| 1000 | Overdriven | MIXER | BREW_CLEANSING_SUPER, BREW_FAST_SUPER, BREW_HEALING_SUPER, BREW_HEALING_SUPER_INSTANT, BREW_RECOVERY_SUPER, BREW_RECOVERY_SUPER_INSTANT, BREW_STRENGTH_SUPER, FOOD_CRITICALING_PIE_RAW, FOOD_EARTHYAM_PIE_RAW, FOOD_ENERGISING_PIE_RAW, FOOD_FLOATING_PIE_RAW, FOOD_INVIGORATING_PIE_RAW, FOOD_MEAT_PIE_RAW, FOOD_PERSISTING_PIE_RAW, FOOD_PROTECTING_PIE_CAUSTIC_RAW, FOOD_PROTECTING_PIE_POTENT_RAW, FOOD_PROTECTING_PIE_VOLATILE_RAW, FOOD_SHIELDING_PIE_RAW, FOOD_STARBERRY_PIE_RAW, FOOD_TEACHING_PIE_RAW, ITEM_OIL_REDUCTION |
| 1000 | Overdriven | REFINERY | CRYSTAL_GLEAM_DECORATIVE, DOOR_ORNATE_STONE, DOOR_STYLISH_METAL, GLASS_DEFAULT_STYLISH, ORNATE_STONE_BEAM, ORNATE_STONE_POLE, PANE_GLASS_STYLISH, ROCK_MARBLE_DECORATIVE_BORDER, SIGN_GLASS_BASE, SIGN_GLASS_MODULAR, SIGN_GLEAM_BASE, SIGN_GLEAM_MODULAR, STORAGE_FARMED_ORNATE, STORAGE_METAL_STYLISH, STORAGE_STONE_ORNATE, STORAGE_WOOD_ORNATE, STYLISH_METAL_BEAM, STYLISH_METAL_POLE, TRAP_ORNATE_STONE, TRAP_STYLISH_METAL |
| 1000 | Overdriven | WORKBENCH | GEM_AMETHYST_DECORATIVE, GEM_DIAMOND_DECORATIVE, GEM_EMERALD_DECORATIVE, GEM_RUBY_DECORATIVE, GEM_SAPPHIRE_DECORATIVE, GEM_TOPAZ_DECORATIVE, ITEM_BREW_CONTAINER4, PORTAL_CONDUIT_BASE |
| 1200 | Overdriven | COMPACTOR | GRAVEL_DEFAULT_BASE_DUGUP, GRAVEL_DEFAULT_COMPACT, METAL_TITANIUM_MACHINED_GRID, SAND_DEFAULT_BASE_DUGUP |
| 1200 | Overdriven | EXTRACTOR | FORGE_SOLVENT_REMOVAL_BOON2, FORGE_SOLVENT_REMOVAL_DEFECT1, FORGE_SOLVENT_REMOVAL_QUIRK1 |
| 1200 | Overdriven | MIXER | BREW_SMOOTHIE_HEALING_SUPER, SAND_CONCRETE_DECORATIVE |
| 1200 | Overdriven | REFINERY | GRAVEL_MOSAIC_DECORATIVE_FISHSCALE, GRAVEL_MOSAIC_DECORATIVE_WAVE, GRAVEL_MOSAIC_REFINED, METAL_TITANIUM_DECORATIVE_FILIGREE, METAL_TITANIUM_MACHINED_CIRCUITBOARD, ROCK_MARBLE_DECORATIVE_FRIEZE0, WICKER |
| 1500 | Overdriven | COMPACTOR | AUGMENT_BOMB_AREAOFEFFECT3, AUGMENT_BOMB_CRITICALEFFECT3, AUGMENT_BOMB_DAMAGE3, AUGMENT_BOMB_PROJECTILESPEED3, AUGMENT_SLINGBOW_ACTIONSPEED3, AUGMENT_SLINGBOW_CRITICALEFFECT3, AUGMENT_SLINGBOW_DAMAGE3, AUGMENT_SLINGBOW_MAXLENGTH3, AUGMENT_SLINGBOW_PROJECTILESPEED3 |
| 1500 | Overdriven | EXTRACTOR | FORGE_CATALYST_POINTS_BOON1, FORGE_CATALYST_VIGOUR1, FORGE_COMPOUND_REDUCTION_DEFECT2, FORGE_COMPOUND_REDUCTION_DEFECT_UNSTABLE2, FORGE_COMPOUND_REDUCTION_QUIRK2, FORGE_COMPOUND_REDUCTION_QUIRK_IMPERFECT2, FORGE_PASTE_REVERSAL_FATE |
| 1500 | Overdriven | MIXER | BUCKET_MILK_FORTIFIED, BUCKET_WATER_FORTIFIED, ITEM_BONDINGAGENT_SUPERENRICHED, ITEM_BUTTER_PURIFIED, ITEM_OIL_REDUCTION |
| 1500 | Overdriven | REFINERY | PHYSICS_SLIDE_BASE, PHYSICS_TRAMPOLINE_BASE |
| 1500 | Overdriven | WORKBENCH | MACHINE_ENGINE_COMPACTORADVANCED, MACHINE_ENGINE_DYE_MAKER, MACHINE_ENGINE_EXTRACTORADVANCED, MACHINE_ENGINE_FORGEADVANCED, MACHINE_ENGINE_MIXERADVANCED, MACHINE_ENGINE_REFINERYADVANCED, MACHINE_ENGINE_WORKBENCHADVANCED, TOOL_AXE_AMETHYST, TOOL_AXE_DIAMOND, TOOL_AXE_EMERALD, TOOL_AXE_RUBY, TOOL_AXE_SAPPHIRE, TOOL_AXE_TOPAZ, TOOL_BAGOFBOMBS_AMETHYST, TOOL_BAGOFBOMBS_DIAMOND, TOOL_BAGOFBOMBS_EMERALD, TOOL_BAGOFBOMBS_RUBY, TOOL_BAGOFBOMBS_SAPPHIRE, TOOL_BAGOFBOMBS_TOPAZ, TOOL_CHISEL_AMETHYST, TOOL_CHISEL_DIAMOND, TOOL_CHISEL_EMERALD, TOOL_CHISEL_RUBY, TOOL_CHISEL_SAPPHIRE, TOOL_CHISEL_TOPAZ, TOOL_GRAPPLE_AMETHYST, TOOL_GRAPPLE_DIAMOND, TOOL_GRAPPLE_EMERALD, TOOL_GRAPPLE_RUBY, TOOL_GRAPPLE_SAPPHIRE, TOOL_GRAPPLE_TOPAZ, TOOL_HAMMER_AMETHYST, TOOL_HAMMER_DIAMOND, TOOL_HAMMER_EMERALD, TOOL_HAMMER_RUBY, TOOL_HAMMER_SAPPHIRE, TOOL_HAMMER_TOPAZ, TOOL_SHOVEL_AMETHYST, TOOL_SHOVEL_DIAMOND, TOOL_SHOVEL_EMERALD, TOOL_SHOVEL_RUBY, TOOL_SHOVEL_SAPPHIRE, TOOL_SHOVEL_TOPAZ, TOOL_SLINGBOW_AMETHYST, TOOL_SLINGBOW_DIAMOND, TOOL_SLINGBOW_EMERALD, TOOL_SLINGBOW_RUBY, TOOL_SLINGBOW_SAPPHIRE, TOOL_SLINGBOW_TOPAZ, TOOL_SPANNER_AMETHYST, TOOL_SPANNER_DIAMOND, TOOL_SPANNER_EMERALD, TOOL_SPANNER_RUBY, TOOL_SPANNER_SAPPHIRE, TOOL_SPANNER_TOPAZ |
| 1600 | Overdriven | MIXER | COAL_HARD_ENRICHED |
| 1600 | Overdriven | WORKBENCH | MACHINE_ENGINE_COIN_MACHINE |
| 1800 | Overdriven | EXTRACTOR | FORGE_PASTE_BOOST_BOON2, FORGE_PASTE_FATE2, FORGE_PASTE_GUARD_DEFECT2, FORGE_PASTE_GUARD_QUIRK2, FORGE_PASTE_PROTECTION2 |
| 2000 | Overdriven | COMPACTOR | COAL_HARD_COMPACT, CROPDROP_FUEL_COALINFUSED, ITEM_AMALGAM_OORTSTONE, ITEM_FUSION_ELEMENTAL |
| 2000 | Overdriven | EXTRACTOR | FORGE_COMPOUND_BOON_IMPERFECT3, FORGE_COMPOUND_BOON_PURE3, FORGE_COMPOUND_BOON_UNSTABLE3, FORGE_PASTE_ENVIGORATION2, FORGE_PASTE_GAMBLING2, FORGE_PASTE_STABILISATION2 |
| 2000 | Overdriven | MIXER | BREW_AGGRAVATING_MEGA, BREW_CALMING_MEGA, BREW_CLEANSING_MEGA, BREW_FAST_MEGA, BREW_HEALING_MEGA, BREW_HEALING_MEGA_INSTANT, BREW_INNOCUOUS_SUPER, BREW_LUMINOUS_SUPER, BREW_RECOVERY_MEGA, BREW_RECOVERY_MEGA_INSTANT, BREW_REVIVAL_BASE, BREW_STRENGTH_MEGA |
| 2000 | Overdriven | REFINERY | DOOR_ORNATE_METAL, DOOR_STYLISH_GLEAM, ORNATE_METAL_BEAM, ORNATE_METAL_POLE, SIGN_GLASS_REFINED, SIGN_GLASS_REFINEDMODULAR, STORAGE_GLEAM_STYLISH, STORAGE_METAL_ORNATE, STYLISH_GLEAM_BEAM, STYLISH_GLEAM_POLE, TRAP_ORNATE_METAL, TRAP_STYLISH_GLEAM |
| 2000 | Overdriven | WORKBENCH | ITEM_BREW_CONTAINER5, MACHINE_GUILD_BUFF_AUGMENT_DURABILITY, MACHINE_GUILD_BUFF_GRAPPLE_AND_RUN, MACHINE_GUILD_BUFF_HAMMER_VERSATILITY, MACHINE_GUILD_BUFF_HOME_HEALING, SPECIAL_BLINK_DECORATIVE, SPECIAL_DARKMATTER_DECORATIVE, SPECIAL_OORTSTONE_DECORATIVE, SPECIAL_RIFT_DECORATIVE |
| 2000 | Supercharged | REFINERY | GLASS_DEFAULT_ORNATE, PANE_GLASS_ORNATE |
| 2500 | Supercharged | EXTRACTOR | FORGE_CATALYST_BOOST_BOON, FORGE_CATALYST_REVERSAL_DEFECT, FORGE_COMPOUND_BOON_CORRUPTED2, FORGE_COMPOUND_BOON_DRAINING2 |
| 2500 | Supercharged | MIXER | BREW_SMOOTHIE_HEALING_MEGA |
| 2600 | Supercharged | EXTRACTOR | FORGE_SOLVENT_REMOVAL_DEFECT2, FORGE_SOLVENT_REMOVAL_QUIRK2, FORGE_SOLVENT_TRANSMUTE_BOON |
| 2600 | Supercharged | WORKBENCH | MACHINE_DYE_MAKER_BASE |
| 3000 | Supercharged | COMPACTOR | SPECIAL_BLINK_COMPACT, SPECIAL_DARKMATTER_COMPACT, SPECIAL_OORTSTONE_COMPACT, SPECIAL_RIFT_COMPACT |
| 3000 | Supercharged | EXTRACTOR | ESSENCE_VITAL_FRESH, FORGE_CATALYST_POINTS_BOON2, FORGE_CATALYST_VIGOUR2, FORGE_RESIN_DECONSTRUCTION3 |
| 3000 | Supercharged | MIXER | ITEM_BUTTER_FORTIFIED |
| 3000 | Supercharged | REFINERY | DOOR_ORNATE_GLEAM, ORNATE_GLEAM_BEAM, ORNATE_GLEAM_POLE, SPECIAL_BLINK_REFINED, SPECIAL_DARKMATTER_REFINED, SPECIAL_OORTSTONE_REFINED, SPECIAL_RIFT_REFINED, TRAP_ORNATE_GLEAM |
| 3000 | Supercharged | WORKBENCH | ITEM_BEACONFUEL_GREATER, MACHINE_GUILD_BUFF_BONUS_CONSUMABLES, MACHINE_GUILD_BUFF_BREW_EXTENSION, MACHINE_GUILD_BUFF_BUILDING_SAFETY, MACHINE_GUILD_BUFF_DEATH_INSURANCE, TOOL_AXE_BLINK, TOOL_AXE_DARK_MATTER, TOOL_AXE_RIFT, TOOL_BAGOFBOMBS_BLINK, TOOL_BAGOFBOMBS_DARK_MATTER, TOOL_BAGOFBOMBS_RIFT, TOOL_GRAPPLE_BLINK, TOOL_GRAPPLE_DARK_MATTER, TOOL_GRAPPLE_RIFT, TOOL_HAMMER_BLINK, TOOL_HAMMER_DARK_MATTER, TOOL_HAMMER_RIFT, TOOL_SHOVEL_BLINK, TOOL_SHOVEL_DARK_MATTER, TOOL_SHOVEL_RIFT, TOOL_SLINGBOW_BLINK, TOOL_SLINGBOW_DARK_MATTER, TOOL_SLINGBOW_RIFT |
| 4000 | Supercharged | MIXER | CROPDROP_FUEL_ENRICHED |
| 4000 | Supercharged | REFINERY | STORAGE_GLEAM_ORNATE |
| 6800 | Supercharged | WORKBENCH | MACHINE_ENGINE_HELIX |

Recipes in BI only (later patch: melee weapons, shields), power > 0:

| Power | Machine | Outputs (display names) |
|---|---|---|
| 1500 | Workbench | Amethyst Shield, Amethyst Swift Sword, Diamond Heavy Sword, Diamond Shield, Emerald Heavy Sword, Emerald Shield, Ruby Balanced Sword, Ruby Shield, Sapphire Balanced Sword, Sapphire Shield, Topaz Shield, Topaz Swift Sword |
| 3000 | Mixer | Second Chance Brew |
| 3000 | Workbench | Blink Shield, Blink Swift Glaive, Rift Heavy Sword, Rift Shield, Umbris Shield |

## 2. Full recipes ([MAY-R]; skill from [BI-R] `skillsNeeded`)

Columns: output and count per single craft; machine; `machineLevel`; `powerRequired`; `duration` single/bulk/mass in s; `spark[0]`; `heat` (furnace); `wear` single/bulk/mass; `craftXP`; skill gate; single-craft inputs. A blank skill means the recipe lists none.

### Metals and alloys
| Output ×n (single) | Machine | Level | Power | Time s/b/m | Spark | Heat | Wear s/b/m | XP | Skill (BI) | Inputs (single) |
|---|---|---|---|---|---|---|---|---|---|---|
| ITEM_METAL_GOLD ×1 | FURNACE | Standard | 0 | 180/1800/9000 |  | 400 | 0/0/0 | 16 |  | 1 ITEM_ORE_GOLD |
| ITEM_METAL_TITANIUM ×1 | FURNACE | Standard | 0 | 240/2400/12000 |  | 600 | 0/0/0 | 22 |  | 1 ITEM_ORE_TITANIUM |
| ITEM_METAL_GOLDALLOY ×1 | MIXER | Powered | 0 | 50/500/2500 | 100 |  | 14/65/210 | 12 |  | 1 ITEM_METAL_GOLD, 1 ITEM_METAL_COPPER |
| ITEM_METAL_TITANIUMALLOY ×1 | MIXER | Powered | 0 | 60/600/3000 | 200 |  | 15/75/210 | 15 |  | 1 ITEM_METAL_TITANIUM, 2 Any Base Alloy |
| METAL_GOLD_COMPACT ×1 | COMPACTOR | Powered | 0 | 110/1100/5500 | 400 |  | 50/330/600 | 21 |  | 10 ITEM_METAL_GOLDALLOY |
| METAL_TITANIUM_COMPACT ×1 | COMPACTOR | Powered | 0 | 120/1200/6000 | 400 |  | 55/350/620 | 22 |  | 10 ITEM_METAL_TITANIUMALLOY |
| METAL_GOLD_REFINED ×1 | REFINERY | Powered | 0 | 50/500/2500 | 400 |  | 35/160/520 | 16 |  | 1 METAL_GOLD_COMPACT |
| METAL_TITANIUM_REFINED ×1 | REFINERY | Powered | 0 | 60/600/3000 | 400 |  | 35/190/540 | 20 |  | 1 METAL_TITANIUM_COMPACT |
| METAL_GOLD_MACHINED ×1 | WORKBENCH | Powered | 0 | 50/500/2500 | 400 |  | 7/30/100 | 17 |  | 1 METAL_GOLD_REFINED |
| METAL_TITANIUM_MACHINED ×1 | WORKBENCH | Powered | 0 | 60/600/3000 | 400 |  | 7/35/110 | 20 |  | 1 METAL_TITANIUM_REFINED |

### Bonding agents, leather, bark, coal, fossil essence
| Output ×n (single) | Machine | Level | Power | Time s/b/m | Spark | Heat | Wear s/b/m | XP | Skill (BI) | Inputs (single) |
|---|---|---|---|---|---|---|---|---|---|---|
| COAL_SOFT_COMPACT ×1 | COMPACTOR | Overdriven | 500 | 120/1200/6000 | 200 |  | 55/350/620 | 12 |  | 10 ITEM_COAL_SOFT |
| COAL_DEFAULT_COMPACT ×1 | COMPACTOR | Overdriven | 1000 | 240/2400/12000 | 400 |  | 85/520/760 | 20 |  | 10 ITEM_COAL_BASE |
| COAL_HARD_COMPACT ×1 | COMPACTOR | Overdriven | 2000 | 480/4800/24000 | 800 |  | 150/590/940 | 40 |  | 10 ITEM_COAL_HARD |
| ESSENCE_VITAL_ANCIENT ×1 | EXTRACTOR | Standard | 0 | 60/300/1500 |  |  | 7/21/85 | 4 |  | 1 ITEM_FOSSIL_SMALL |
| ESSENCE_VITAL_ANCIENT ×4 | EXTRACTOR | Powered | 0 | 240/1200/6000 | 100 |  | 17/70/120 | 8 |  | 1 ITEM_FOSSIL_MEDIUM |
| ESSENCE_VITAL_ANCIENT ×10 | EXTRACTOR | Overdriven | 500 | 600/3000/15000 | 200 |  | 35/110/160 | 12 |  | 1 ITEM_FOSSIL_LARGE |
| ITEM_LEATHER_BASE ×1 | REFINERY | Overdriven | 500 | 60/600/3000 | 200 |  | 35/190/540 | 12 |  | 4 ITEM_HIDE_BASE, 4 ITEM_BARK_BASE |
| ITEM_LEATHER_HARD ×1 | FURNACE | Standard | 0 | 120/1200/6000 |  | 400 | 0/0/0 | 15 |  | 2 ITEM_LEATHER_BASE, 2 BUCKET_WATER_BASE |
| ITEM_BONDINGAGENT_BASE ×1 | MIXER | Standard | 0 | 30/300/1500 |  |  | 11/40/170 | 6 |  | 2 ITEM_GLUE_BASE, 2 ESSENCE_VITAL_ANCIENT, 1 ESSENCE_VITAL_FRESH, 1 ITEM_ORB_PULSATING |
| ITEM_BONDINGAGENT_ENRICHED ×1 | MIXER | Powered | 0 | 60/600/3000 | 200 |  | 15/75/210 | 16 |  | 4 ITEM_GLUE_BASE, 10 ESSENCE_VITAL_ANCIENT, 5 ESSENCE_VITAL_FRESH, 1 ITEM_ORB_SHIMMERING |
| ITEM_BONDINGAGENT_SUPERENRICHED ×1 | MIXER | Overdriven | 1500 | 90/900/4500 | 400 |  | 18/110/230 | 26 |  | 8 ITEM_GLUE_BASE, 60 ESSENCE_VITAL_ANCIENT, 30 ESSENCE_VITAL_FRESH, 1 ITEM_ORB_SHADOW |
| ITEM_BARK_BASE ×1 | EXTRACTOR | Overdriven | 500 | 120/1200/6000 | 100 |  | 11/70/120 | 8 | Brew Crafting Level 1 | 2 ITEM_CROPDROP_NUTS |
| COAL_SOFT_ENRICHED ×1 | MIXER | Overdriven | 200 | 300/3000/15000 | 200 |  | 40/210/330 | 24 |  | 1 COAL_SOFT_COMPACT, 5 ITEM_CROPDROP_FUEL_ENRICHER |
| COAL_DEFAULT_ENRICHED ×1 | MIXER | Overdriven | 600 | 300/3000/15000 | 400 |  | 40/210/330 | 28 |  | 1 COAL_DEFAULT_COMPACT, 5 ITEM_CROPDROP_FUEL_ENRICHER |
| COAL_HARD_ENRICHED ×1 | MIXER | Overdriven | 1600 | 300/3000/15000 | 800 |  | 40/210/330 | 36 |  | 1 COAL_HARD_COMPACT, 5 ITEM_CROPDROP_FUEL_ENRICHER |

### Silver, gold and titanium gear (hammer, shovel and axe share one recipe; shown once as the hammer)
| Output ×n (single) | Machine | Level | Power | Time s/b/m | Spark | Heat | Wear s/b/m | XP | Skill (BI) | Inputs (single) |
|---|---|---|---|---|---|---|---|---|---|---|
| TOOL_HAMMER_SILVER ×1 | WORKBENCH | Powered | 0 | 240/720/2400 | 800 |  | 17/45/100 | 40 | Tool Crafting Level 2 | 3 ITEM_METAL_SILVERALLOY, 4 ITEM_STICK_BASE, 1 ITEM_CLOTH_COTTON, 1 ITEM_BONDINGAGENT_BASE |
| TOOL_CHISEL_SILVER ×1 | WORKBENCH | Powered | 0 | 240/720/2400 | 800 |  | 17/45/100 | 40 | Tech Crafting Level 2 | 3 ITEM_METAL_SILVERALLOY, 4 ITEM_STICK_BASE, 1 ITEM_CLOTH_COTTON, 1 ITEM_BONDINGAGENT_BASE |
| TOOL_GRAPPLE_SILVER ×1 | WORKBENCH | Powered | 0 | 240/720/2400 | 800 |  | 17/45/100 | 40 | Tech Crafting Level 2 | 3 ITEM_METAL_SILVERALLOY, 1 ITEM_CLOTH_COTTON, 1 ITEM_BONDINGAGENT_BASE, 5 ITEM_ANCIENTTECHNOLOGY_FRAGMENT |
| TOOL_SPANNER_SILVER ×1 | WORKBENCH | Powered | 0 | 240/720/2400 | 800 |  | 17/45/100 | 40 | Tech Crafting Level 2 | 3 ITEM_METAL_SILVERALLOY, 4 ITEM_STICK_BASE, 1 ITEM_CLOTH_COTTON, 1 ITEM_BONDINGAGENT_BASE, 4 ITEM_COG_BASE |
| TOOL_HAMMER_GOLD ×1 | WORKBENCH | Powered | 0 | 480/1440/4800 | 800 |  | 30/85/120 | 56 | Tool Crafting Level 2 | 3 ITEM_METAL_GOLDALLOY, 4 ITEM_STICK_BASE, 1 ITEM_CLOTH_COTTON, 1 ITEM_BONDINGAGENT_BASE |
| TOOL_CHISEL_GOLD ×1 | WORKBENCH | Powered | 0 | 480/1440/4800 | 800 |  | 30/85/120 | 56 | Tech Crafting Level 2 | 3 ITEM_METAL_GOLDALLOY, 4 ITEM_STICK_BASE, 1 ITEM_CLOTH_COTTON, 1 ITEM_BONDINGAGENT_BASE |
| TOOL_GRAPPLE_GOLD ×1 | WORKBENCH | Powered | 0 | 480/1440/4800 | 800 |  | 30/85/120 | 56 | Tech Crafting Level 2 | 3 ITEM_METAL_GOLDALLOY, 1 ITEM_CLOTH_COTTON, 1 ITEM_BONDINGAGENT_BASE, 5 ITEM_ANCIENTTECHNOLOGY_FRAGMENT |
| TOOL_SPANNER_GOLD ×1 | WORKBENCH | Powered | 0 | 480/1440/4800 | 800 |  | 30/85/120 | 56 | Tech Crafting Level 2 | 3 ITEM_METAL_GOLDALLOY, 4 ITEM_STICK_BASE, 1 ITEM_CLOTH_COTTON, 1 ITEM_BONDINGAGENT_BASE, 4 ITEM_COG_BASE |
| TOOL_HAMMER_TITANIUM ×1 | WORKBENCH | Powered | 0 | 1200/3600/12000 | 1500 |  | 70/110/150 | 100 | Tool Crafting Level 2 | 3 ITEM_METAL_TITANIUMALLOY, 4 ITEM_STICK_BASE, 1 ITEM_CLOTH_COTTON, 1 ITEM_BONDINGAGENT_ENRICHED |
| TOOL_CHISEL_TITANIUM ×1 | WORKBENCH | Powered | 0 | 1200/3600/12000 | 1500 |  | 70/110/150 | 100 | Tech Crafting Level 2 | 3 ITEM_METAL_TITANIUMALLOY, 4 ITEM_STICK_BASE, 1 ITEM_CLOTH_COTTON, 1 ITEM_BONDINGAGENT_ENRICHED |
| TOOL_GRAPPLE_TITANIUM ×1 | WORKBENCH | Powered | 0 | 1200/3600/12000 | 1500 |  | 70/110/150 | 100 | Tech Crafting Level 2 | 3 ITEM_METAL_TITANIUMALLOY, 1 ITEM_CLOTH_COTTON, 1 ITEM_BONDINGAGENT_ENRICHED, 1 ITEM_ANCIENTTECHNOLOGY_COMPONENT, 5 ITEM_ANCIENTTECHNOLOGY_FRAGMENT |
| TOOL_SPANNER_TITANIUM ×1 | WORKBENCH | Powered | 0 | 1200/3600/12000 | 1500 |  | 70/110/150 | 100 | Tech Crafting Level 2 | 3 ITEM_METAL_TITANIUMALLOY, 4 ITEM_STICK_BASE, 1 ITEM_CLOTH_COTTON, 1 ITEM_BONDINGAGENT_ENRICHED, 4 ITEM_COG_BASE |

### Gems and gem gear

All six gems' compact, refined, tool, chisel and spanner recipes are identical except the gem; grapples differ for topaz and sapphire (shown).

| Output ×n (single) | Machine | Level | Power | Time s/b/m | Spark | Heat | Wear s/b/m | XP | Skill (BI) | Inputs (single) |
|---|---|---|---|---|---|---|---|---|---|---|
| GEM_DIAMOND_COMPACT ×1 | COMPACTOR | Powered | 0 | 240/2400/12000 | 800 |  | 85/520/760 | 38 |  | 10 ITEM_ROUGH_DIAMOND |
| ITEM_REFINED_DIAMOND ×1 | REFINERY | Powered | 0 | 120/1200/6000 | 800 |  | 55/350/620 | 30 |  | 1 GEM_DIAMOND_COMPACT |
| TOOL_HAMMER_DIAMOND ×1 | WORKBENCH | Overdriven | 1500 | 1800/5400/18000 | 2000 |  | 100/120/170 | 160 | Tool Crafting Level 3 | 3 ITEM_REFINED_DIAMOND, 4 ITEM_STICK_BASE, 1 ITEM_LEATHER_BASE, 1 ITEM_BONDINGAGENT_ENRICHED |
| TOOL_CHISEL_DIAMOND ×1 | WORKBENCH | Overdriven | 1500 | 1800/5400/18000 | 2000 |  | 100/120/170 | 160 | Tech Crafting Level 3 | 3 ITEM_REFINED_DIAMOND, 4 ITEM_STICK_BASE, 1 ITEM_LEATHER_BASE, 1 ITEM_BONDINGAGENT_ENRICHED |
| TOOL_GRAPPLE_DIAMOND ×1 | WORKBENCH | Overdriven | 1500 | 1800/5400/18000 | 2000 |  | 100/120/170 | 160 | Tech Crafting Level 3 | 3 ITEM_REFINED_DIAMOND, 1 ITEM_LEATHER_BASE, 1 ITEM_BONDINGAGENT_ENRICHED, 1 ITEM_ANCIENTTECHNOLOGY_COMPONENT, 5 ITEM_ANCIENTTECHNOLOGY_FRAGMENT |
| TOOL_SPANNER_DIAMOND ×1 | WORKBENCH | Overdriven | 1500 | 1800/5400/18000 | 2000 |  | 100/120/170 | 160 | Tech Crafting Level 3 | 3 ITEM_REFINED_DIAMOND, 4 ITEM_STICK_BASE, 1 ITEM_LEATHER_BASE, 1 ITEM_BONDINGAGENT_ENRICHED, 6 ITEM_COG_BASE |
| TOOL_GRAPPLE_TOPAZ ×1 | WORKBENCH | Overdriven | 1500 | 1800/5400/18000 | 2000 |  | 100/120/170 | 160 | Tech Crafting Level 3 | 3 ITEM_REFINED_TOPAZ, 1 ITEM_LEATHER_BASE, 1 ITEM_BONDINGAGENT_ENRICHED, 1 ITEM_ANCIENTTECHNOLOGY_COMPONENT, 1 ITEM_ANCIENTTECHNOLOGY_FRAGMENT |
| TOOL_GRAPPLE_SAPPHIRE ×1 | WORKBENCH | Overdriven | 1500 | 1800/5400/18000 | 2000 |  | 100/120/170 | 160 | Tech Crafting Level 3 | 3 ITEM_REFINED_SAPPHIRE, 1 ITEM_LEATHER_BASE, 1 ITEM_BONDINGAGENT_ENRICHED, 1 ITEM_ANCIENTTECHNOLOGY_COMPONENT, 1 ITEM_ANCIENTTECHNOLOGY_FRAGMENT |

### Power coils and the titanium furnace
| Output ×n (single) | Machine | Level | Power | Time s/b/m | Spark | Heat | Wear s/b/m | XP | Skill (BI) | Inputs (single) |
|---|---|---|---|---|---|---|---|---|---|---|
| MACHINE_FIREPIT_TITANIUM ×1 | WORKBENCH | Powered | 0 | 180/900/4500 | 400 |  | 14/55/120 | 26 | Machine Crafting Level 2 | 6 ITEM_METAL_TITANIUMALLOY, 16 ITEM_ROCKDROP_FLINT |
| MACHINE_CRUCIBLE_TITANIUM ×1 | WORKBENCH | Powered | 0 | 180/900/4500 | 400 |  | 14/55/120 | 26 | Machine Crafting Level 2 | 6 ITEM_METAL_TITANIUMALLOY |
| MACHINE_ENGINE_WORKBENCH ×1 | WORKBENCH | Powered | 0 | 900/4500/22500 | 800 |  | 55/120/180 | 90 | Machine Crafting Level 2 | 1 MACHINE_FIREPIT_IRON, 3 METAL_COPPER_MACHINED, 1 GEM_DIAMOND_COMPACT, 3 ITEM_ANCIENTTECHNOLOGY_COMPONENT |
| MACHINE_ENGINE_WORKBENCHADVANCED ×1 | WORKBENCH | Overdriven | 1500 | 1800/9000/45000 | 2000 |  | 100/140/200 | 180 | Machine Crafting Level 3 | 1 MACHINE_FIREPIT_TITANIUM, 3 METAL_SILVER_MACHINED, 2 ITEM_REFINED_DIAMOND, 6 ITEM_ANCIENTTECHNOLOGY_COMPONENT, 1 ITEM_ANCIENTTECHNOLOGY_DEVICE |
| MACHINE_ENGINE_EXTRACTOR ×1 | WORKBENCH | Powered | 0 | 900/4500/22500 | 800 |  | 55/120/180 | 90 | Machine Crafting Level 2 | 1 MACHINE_FIREPIT_IRON, 3 METAL_COPPER_MACHINED, 1 GEM_SAPPHIRE_COMPACT, 3 ITEM_ANCIENTTECHNOLOGY_COMPONENT |
| MACHINE_ENGINE_EXTRACTORADVANCED ×1 | WORKBENCH | Overdriven | 1500 | 1800/9000/45000 | 2000 |  | 100/140/200 | 180 | Machine Crafting Level 3 | 1 MACHINE_FIREPIT_TITANIUM, 3 METAL_SILVER_MACHINED, 2 ITEM_REFINED_SAPPHIRE, 6 ITEM_ANCIENTTECHNOLOGY_COMPONENT, 1 ITEM_ANCIENTTECHNOLOGY_DEVICE |
| MACHINE_ENGINE_COMPACTOR ×1 | WORKBENCH | Powered | 0 | 900/4500/22500 | 800 |  | 55/120/180 | 90 | Machine Crafting Level 2 | 1 MACHINE_FIREPIT_IRON, 3 METAL_IRON_MACHINED, 1 GEM_RUBY_COMPACT, 3 ITEM_ANCIENTTECHNOLOGY_COMPONENT |
| MACHINE_ENGINE_COMPACTORADVANCED ×1 | WORKBENCH | Overdriven | 1500 | 1800/9000/45000 | 2000 |  | 100/140/200 | 180 | Machine Crafting Level 3 | 1 MACHINE_FIREPIT_TITANIUM, 3 METAL_GOLD_MACHINED, 2 ITEM_REFINED_RUBY, 6 ITEM_ANCIENTTECHNOLOGY_COMPONENT, 1 ITEM_ANCIENTTECHNOLOGY_DEVICE |
| MACHINE_ENGINE_REFINERY ×1 | WORKBENCH | Powered | 0 | 900/4500/22500 | 800 |  | 55/120/180 | 90 | Machine Crafting Level 2 | 1 MACHINE_FIREPIT_IRON, 3 METAL_IRON_MACHINED, 1 GEM_TOPAZ_COMPACT, 3 ITEM_ANCIENTTECHNOLOGY_COMPONENT |
| MACHINE_ENGINE_REFINERYADVANCED ×1 | WORKBENCH | Overdriven | 1500 | 1800/9000/45000 | 2000 |  | 100/140/200 | 180 | Machine Crafting Level 3 | 1 MACHINE_FIREPIT_TITANIUM, 3 METAL_GOLD_MACHINED, 2 ITEM_REFINED_TOPAZ, 6 ITEM_ANCIENTTECHNOLOGY_COMPONENT, 1 ITEM_ANCIENTTECHNOLOGY_DEVICE |
| MACHINE_ENGINE_MIXER ×1 | WORKBENCH | Powered | 0 | 900/4500/22500 | 800 |  | 55/120/180 | 90 | Machine Crafting Level 2 | 1 MACHINE_FIREPIT_IRON, 3 METAL_COPPER_MACHINED, 1 GEM_EMERALD_COMPACT, 3 ITEM_ANCIENTTECHNOLOGY_COMPONENT |
| MACHINE_ENGINE_MIXERADVANCED ×1 | WORKBENCH | Overdriven | 1500 | 1800/9000/45000 | 2000 |  | 100/140/200 | 180 | Machine Crafting Level 3 | 1 MACHINE_FIREPIT_TITANIUM, 3 METAL_SILVER_MACHINED, 2 ITEM_REFINED_EMERALD, 6 ITEM_ANCIENTTECHNOLOGY_COMPONENT, 1 ITEM_ANCIENTTECHNOLOGY_DEVICE |
| MACHINE_ENGINE_FORGE ×1 | WORKBENCH | Powered | 0 | 900/4500/22500 | 800 |  | 55/120/180 | 90 | Machine Crafting Level 2 | 1 MACHINE_FIREPIT_IRON, 3 METAL_GOLD_MACHINED, 1 GEM_AMETHYST_COMPACT, 3 ITEM_ANCIENTTECHNOLOGY_COMPONENT |
| MACHINE_ENGINE_FORGEADVANCED ×1 | WORKBENCH | Overdriven | 1500 | 1800/9000/45000 | 2000 |  | 100/140/200 | 180 | Machine Crafting Level 3 | 1 MACHINE_FIREPIT_TITANIUM, 3 METAL_TITANIUM_MACHINED, 2 ITEM_REFINED_AMETHYST, 6 ITEM_ANCIENTTECHNOLOGY_COMPONENT, 1 ITEM_ANCIENTTECHNOLOGY_DEVICE |

### Atmosphere food (all ranks, all three lines) and amalgams
| Output ×n (single) | Machine | Level | Power | Time s/b/m | Spark | Heat | Wear s/b/m | XP | Skill (BI) | Inputs (single) |
|---|---|---|---|---|---|---|---|---|---|---|
| ITEM_AMALGAM_CORROSION ×1 | COMPACTOR | Powered | 0 | 90/900/4500 | 200 |  | 45/270/580 | 22 | Food Crafting Level 1 | 1 ITEM_SHARD_CORROSION, 1 ITEM_BLOOD_FRESH |
| ITEM_AMALGAM_TOXIN ×1 | COMPACTOR | Powered | 0 | 90/900/4500 | 200 |  | 45/270/580 | 22 | Food Crafting Level 1 | 1 ITEM_SHARD_TOXIN, 1 ITEM_BLOOD_FRESH |
| ITEM_AMALGAM_BLAST ×1 | COMPACTOR | Powered | 0 | 90/900/4500 | 200 |  | 45/270/580 | 22 | Food Crafting Level 1 | 1 ITEM_SHARD_BLAST, 1 ITEM_BLOOD_FRESH |
| ITEM_AMALGAM_BURN ×1 | COMPACTOR | Powered | 0 | 90/900/4500 | 200 |  | 45/270/580 | 22 | Food Crafting Level 1 | 1 ITEM_SHARD_BURN, 1 ITEM_BLOOD_FRESH |
| ITEM_AMALGAM_SHOCK ×1 | COMPACTOR | Powered | 0 | 90/900/4500 | 200 |  | 45/270/580 | 22 | Food Crafting Level 1 | 1 ITEM_SHARD_SHOCK, 1 ITEM_BLOOD_FRESH |
| ITEM_AMALGAM_CHILL ×1 | COMPACTOR | Powered | 0 | 90/900/4500 | 200 |  | 45/270/580 | 22 | Food Crafting Level 1 | 1 ITEM_SHARD_CHILL, 1 ITEM_BLOOD_FRESH |
| FOOD_PROTECTING_CHILLI_VOLATILE_RAW ×1 | MIXER | Standard | 0 | 120/1200/6000 |  |  | 21/140/250 | 12 | Food Crafting Level 1 | 2 FOOD_MEAT_PRIME_RAW, 2 ITEM_BEAN_SPICY, 1 Any Volatile Amalgam, 2 FOOD_EARTHYAM_BASE_RAW |
| FOOD_PROTECTING_BIRYANI_VOLATILE_RAW ×1 | MIXER | Powered | 0 | 160/1600/8000 | 200 |  | 26/190/270 | 16 | Food Crafting Level 3 | 5 FOOD_MEAT_PRIME_RAW, 4 ITEM_BEAN_SPICY, 2 Any Volatile Amalgam, 2 ITEM_BAG_RICE |
| FOOD_PROTECTING_PIE_VOLATILE_RAW ×1 | MIXER | Overdriven | 1000 | 180/1800/9000 | 400 |  | 28/200/280 | 22 | Food Crafting Level 5 | 10 FOOD_MEAT_PRIME_RAW, 8 ITEM_BEAN_SPICY, 4 Any Volatile Amalgam, 2 ITEM_BAG_FLOUR, 2 ITEM_BUTTER_PURIFIED |
| FOOD_PROTECTING_CHILLI_VOLATILE_COOKED ×1 | FURNACE | Standard | 0 | 240/2400/12000 |  | 100 | 0/0/0 | 18 | Food Crafting Level 1 | 1 FOOD_PROTECTING_CHILLI_VOLATILE_RAW |
| FOOD_PROTECTING_BIRYANI_VOLATILE_COOKED ×1 | FURNACE | Standard | 0 | 360/3600/18000 |  | 200 | 0/0/0 | 28 | Food Crafting Level 3 | 1 FOOD_PROTECTING_BIRYANI_VOLATILE_RAW |
| FOOD_PROTECTING_PIE_VOLATILE_COOKED ×1 | FURNACE | Standard | 0 | 600/6000/30000 |  | 300 | 0/0/0 | 44 | Food Crafting Level 5 | 1 FOOD_PROTECTING_PIE_VOLATILE_RAW |
| FOOD_PROTECTING_CHILLI_POTENT_RAW ×1 | MIXER | Standard | 0 | 120/1200/6000 |  |  | 21/140/250 | 12 | Food Crafting Level 1 | 2 FOOD_MEAT_PRIME_RAW, 2 ITEM_BEAN_SPICY, 1 Any Potent Amalgam, 2 FOOD_EARTHYAM_BASE_RAW, 1 ITEM_ROCKDROP_ROCKSALT |
| FOOD_PROTECTING_BIRYANI_POTENT_RAW ×1 | MIXER | Powered | 0 | 160/1600/8000 | 200 |  | 26/190/270 | 18 | Food Crafting Level 3 | 5 FOOD_MEAT_PRIME_RAW, 4 ITEM_BEAN_SPICY, 2 Any Potent Amalgam, 2 ITEM_BAG_RICE, 1 ITEM_ROCKDROP_ROCKSALT |
| FOOD_PROTECTING_PIE_POTENT_RAW ×1 | MIXER | Overdriven | 1000 | 180/1800/9000 | 400 |  | 28/200/280 | 22 | Food Crafting Level 5 | 10 FOOD_MEAT_PRIME_RAW, 8 ITEM_BEAN_SPICY, 4 Any Potent Amalgam, 2 ITEM_BAG_FLOUR, 2 ITEM_BUTTER_PURIFIED, 1 ITEM_ROCKDROP_ROCKSALT |
| FOOD_PROTECTING_CHILLI_POTENT_COOKED ×1 | FURNACE | Standard | 0 | 240/2400/12000 |  | 100 | 0/0/0 | 18 | Food Crafting Level 1 | 1 FOOD_PROTECTING_CHILLI_POTENT_RAW |
| FOOD_PROTECTING_BIRYANI_POTENT_COOKED ×1 | FURNACE | Standard | 0 | 360/3600/18000 |  | 200 | 0/0/0 | 28 | Food Crafting Level 3 | 1 FOOD_PROTECTING_BIRYANI_POTENT_RAW |
| FOOD_PROTECTING_PIE_POTENT_COOKED ×1 | FURNACE | Standard | 0 | 600/6000/30000 |  | 300 | 0/0/0 | 44 | Food Crafting Level 5 | 1 FOOD_PROTECTING_PIE_POTENT_RAW |
| FOOD_PROTECTING_CHILLI_CAUSTIC_RAW ×1 | MIXER | Standard | 0 | 120/1200/6000 |  |  | 21/140/250 | 12 | Food Crafting Level 1 | 2 FOOD_MEAT_PRIME_RAW, 2 ITEM_BEAN_SPICY, 1 Any Caustic Amalgam, 2 FOOD_EARTHYAM_BASE_RAW, 1 ITEM_ROCKDROP_ROCKSALT |
| FOOD_PROTECTING_BIRYANI_CAUSTIC_RAW ×1 | MIXER | Powered | 0 | 160/1600/8000 | 200 |  | 26/190/270 | 18 | Food Crafting Level 3 | 5 FOOD_MEAT_PRIME_RAW, 4 ITEM_BEAN_SPICY, 2 Any Caustic Amalgam, 2 ITEM_BAG_RICE, 1 ITEM_ROCKDROP_ROCKSALT |
| FOOD_PROTECTING_PIE_CAUSTIC_RAW ×1 | MIXER | Overdriven | 1000 | 180/1800/9000 | 400 |  | 28/200/280 | 22 | Food Crafting Level 5 | 10 FOOD_MEAT_PRIME_RAW, 8 ITEM_BEAN_SPICY, 4 Any Caustic Amalgam, 2 ITEM_BAG_FLOUR, 2 ITEM_BUTTER_PURIFIED, 1 ITEM_ROCKDROP_ROCKSALT |
| FOOD_PROTECTING_CHILLI_CAUSTIC_COOKED ×1 | FURNACE | Standard | 0 | 240/2400/12000 |  | 100 | 0/0/0 | 18 | Food Crafting Level 1 | 1 FOOD_PROTECTING_CHILLI_CAUSTIC_RAW |
| FOOD_PROTECTING_BIRYANI_CAUSTIC_COOKED ×1 | FURNACE | Standard | 0 | 360/3600/18000 |  | 200 | 0/0/0 | 28 | Food Crafting Level 3 | 1 FOOD_PROTECTING_BIRYANI_CAUSTIC_RAW |
| FOOD_PROTECTING_PIE_CAUSTIC_COOKED ×1 | FURNACE | Standard | 0 | 600/6000/30000 |  | 300 | 0/0/0 | 44 | Food Crafting Level 5 | 1 FOOD_PROTECTING_PIE_CAUSTIC_RAW |

Reading: rank 1 chilli = 2 raw prime meat, 2 spicy beans, 1 amalgam of the line, 2 raw tubers (+1 rock salt for Caustic and Potent), mixed (120 s, no spark) then cooked (furnace 240 s, heat 100). Rank 2 biryani = 5 prime meat, 4 spicy beans, 2 amalgam, 2 rice (+ rock salt), mixer 160 s 200 spark, furnace 360 s heat 200. Rank 3 pie = 10 prime meat, 8 spicy beans, 4 amalgam, 2 flour, 2 purified butter (+ rock salt), mixer 180 s 400 spark **1,000 power**, furnace 600 s heat 300. Amalgam = 1 elemental shard + 1 fresh blood (compactor, 90 s, 200 spark); elemental shards are creature drops (`categories: CREATURE_DROP, ELEMENTAL_SHARD`), dropped only by elemental creatures (tuning 3, drop tables, from the wiki; medium), so every atmosphere food needs elemental creatures from tier 5 gem worlds. The buff's amount and length are not in these files (bundle names only); tuning 3.9's +1 / +2 / +3 for 30 min stands.

### Buckets, butter, pies
| Output ×n (single) | Machine | Level | Power | Time s/b/m | Spark | Heat | Wear s/b/m | XP | Skill (BI) | Inputs (single) |
|---|---|---|---|---|---|---|---|---|---|---|
| BUCKET_EMPTY_BASE ×1 | WORKBENCH | Standard | 0 | 30/300/1500 |  |  | 6/21/85 | 6 |  | 1 Any Timber, 6 ITEM_NAIL_BASE |
| BUCKET_WATER_PURIFIED ×1 | EXTRACTOR | Powered | 0 | 60/300/1500 | 100 |  | 7/21/85 | 10 |  | 1 BUCKET_WATER_BASE, 4 ITEM_CHARCOAL_BASE, 2 GRAVEL_DEFAULT_BASE_DUGUP |
| BUCKET_WATER_FORTIFIED ×1 | MIXER | Overdriven | 1500 | 120/600/3000 | 200 |  | 21/75/210 | 18 |  | 1 BUCKET_WATER_PURIFIED, 1 ITEM_ORB_SHIMMERING |
| BUCKET_MILK_BASE ×1 | EXTRACTOR | Standard | 0 | 60/300/1500 |  |  | 7/21/85 | 8 |  | 1 ITEM_GLAND_MILK, 1 BUCKET_EMPTY_BASE |
| BUCKET_MILK_PURIFIED ×1 | EXTRACTOR | Powered | 0 | 90/450/2250 | 100 |  | 9/29/100 | 12 |  | 1 BUCKET_MILK_BASE, 4 ITEM_CHARCOAL_BASE, 2 GRAVEL_DEFAULT_BASE_DUGUP |
| BUCKET_MILK_FORTIFIED ×1 | MIXER | Overdriven | 1500 | 120/600/3000 | 200 |  | 21/75/210 | 18 |  | 1 BUCKET_MILK_PURIFIED, 1 ITEM_ORB_SHIMMERING |
| ITEM_BUTTER_BASE ×1 | MIXER | Powered | 0 | 60/600/3000 | 100 |  | 15/75/210 | 8 | Food Crafting Level 5 | 1 BUCKET_MILK_BASE |
| ITEM_BUTTER_PURIFIED ×1 | MIXER | Overdriven | 1500 | 90/900/4500 | 200 |  | 18/110/230 | 14 | Food Crafting Level 5 | 1 BUCKET_MILK_PURIFIED |
| ITEM_BUTTER_FORTIFIED ×1 | MIXER | Supercharged | 3000 | 120/1200/6000 | 400 |  | 21/140/250 | 20 | Food Crafting Level 5 | 1 BUCKET_MILK_FORTIFIED |
| FOOD_STARBERRY_PIE_RAW ×1 | MIXER | Overdriven | 1000 | 120/1200/6000 |  |  | 21/140/250 | 12 | Food Crafting Level 5 | 10 FOOD_STARBERRY_RAW, 1 ITEM_BAG_FLOUR, 1 ITEM_BUTTER_BASE, 1 ITEM_BAG_SUGAR |
| FOOD_EARTHYAM_PIE_RAW ×1 | MIXER | Overdriven | 1000 | 160/1600/8000 | 400 |  | 26/190/270 | 20 | Food Crafting Level 5 | 10 FOOD_EARTHYAM_BASE_RAW, 1 ITEM_BAG_FLOUR, 2 ITEM_BUTTER_BASE, 2 ITEM_ROCKDROP_ROCKSALT |
| FOOD_MEAT_PIE_RAW ×1 | MIXER | Overdriven | 1000 | 180/1800/9000 | 400 |  | 28/200/280 | 20 | Food Crafting Level 5 | 10 FOOD_MEAT_BASE_RAW, 1 ITEM_BAG_FLOUR, 2 ITEM_BUTTER_PURIFIED, 2 ITEM_ROCKDROP_ROCKSALT |
| FOOD_INVIGORATING_PIE_RAW ×1 | MIXER | Overdriven | 1000 | 160/1600/8000 | 400 |  | 26/190/270 | 20 | Food Crafting Level 5 | 10 FOOD_STARBERRY_GLOSSY_RAW, 1 ITEM_BAG_FLOUR, 2 ITEM_BUTTER_PURIFIED, 2 ITEM_BAG_SUGAR |
| FOOD_ENERGISING_PIE_RAW ×1 | MIXER | Overdriven | 1000 | 160/1600/8000 | 400 |  | 26/190/270 | 20 | Food Crafting Level 5 | 10 FOOD_EARTHYAM_WAXY_RAW, 1 ITEM_BAG_FLOUR, 2 ITEM_BUTTER_PURIFIED, 2 ITEM_ROCKDROP_ROCKSALT |
| FOOD_SHIELDING_PIE_RAW ×1 | MIXER | Overdriven | 1000 | 180/1800/9000 | 400 |  | 28/200/280 | 24 | Food Crafting Level 5 | 10 FOOD_MEAT_PRIME_RAW, 2 ITEM_BAG_FLOUR, 3 ITEM_BUTTER_FORTIFIED, 3 ITEM_ROCKDROP_ROCKSALT |
| FOOD_FLOATING_PIE_RAW ×1 | MIXER | Overdriven | 1000 | 160/1600/8000 | 400 |  | 26/190/270 | 20 | Food Crafting Level 5 | 10 FOOD_STARBERRY_JUICY_RAW, 1 ITEM_BAG_FLOUR, 2 ITEM_BUTTER_FORTIFIED, 2 ITEM_BAG_SUGAR |
| FOOD_TEACHING_PIE_RAW ×1 | MIXER | Overdriven | 1000 | 180/1800/9000 | 400 |  | 28/200/280 | 22 | Food Crafting Level 5 | 16 ITEM_BEAN_SWEET, 2 ITEM_BAG_SUGAR, 2 ITEM_BAG_FLOUR, 2 ITEM_BUTTER_PURIFIED |
| FOOD_CRITICALING_PIE_RAW ×1 | MIXER | Overdriven | 1000 | 160/1600/8000 | 400 |  | 26/190/270 | 20 | Food Crafting Level 5 | 10 FOOD_MEAT_LEAN_RAW, 1 ITEM_BAG_FLOUR, 2 ITEM_BUTTER_FORTIFIED, 2 ITEM_ROCKDROP_ROCKSALT |
| FOOD_PERSISTING_PIE_RAW ×1 | MIXER | Overdriven | 1000 | 160/1600/8000 | 400 |  | 26/190/270 | 18 | Food Crafting Level 5 | 10 FOOD_EARTHYAM_EXOTIC_RAW, 1 ITEM_BAG_FLOUR, 2 ITEM_BUTTER_FORTIFIED, 2 ITEM_ROCKDROP_ROCKSALT |
| BUCKET_MILK_BASE ×1 | EXTRACTOR | Standard | 0 | 60/300/1500 |  |  | 7/21/85 | 8 |  | 10 ITEM_CROPDROP_OATS, 1 BUCKET_WATER_BASE |

The cooked pies are furnace recipes (600 s, heat 300, no power). Butter sits at Food Crafting 5 in [BI-R] even for plain butter.

### Forge ingredients (every one; power is the gate)
| Output ×n (single) | Machine | Level | Power | Time s/b/m | Spark | Heat | Wear s/b/m | XP | Skill (BI) | Inputs (single) |
|---|---|---|---|---|---|---|---|---|---|---|
| FORGE_BASE_COMPOUND ×1 | EXTRACTOR | Standard | 0 | 30/300/1500 |  |  | 6/21/85 | 4 | Forge Ingredient Crafting Level 1 | 1 GLASS_DEFAULT_BASE, 1 ITEM_LAMELLA_REACTIVE, 1 ITEM_CHARCOAL_BASE |
| FORGE_BASE_PASTE ×1 | EXTRACTOR | Powered | 0 | 30/300/1500 | 200 |  | 6/21/85 | 8 | Forge Ingredient Crafting Level 1 | 1 GLASS_DEFAULT_BASE, 2 ITEM_BEAN_SPICY, 1 ITEM_BEAN_BITTER |
| FORGE_BASE_GUM ×1 | EXTRACTOR | Standard | 0 | 30/300/1500 |  |  | 6/21/85 | 4 | Forge Ingredient Crafting Level 1 | 1 GLASS_DEFAULT_BASE, 1 ITEM_SYRUP_BASE, 1 ITEM_LEAF_FLESHY |
| FORGE_BASE_CATALYST ×1 | EXTRACTOR | Powered | 0 | 30/300/1500 | 200 |  | 6/21/85 | 8 | Forge Ingredient Crafting Level 2 | 1 GLASS_DEFAULT_BASE, 2 ITEM_LEAF_INKY, 1 ITEM_FRAGMENT_COBALT |
| FORGE_BASE_SOLVENT ×1 | EXTRACTOR | Powered | 0 | 30/300/1500 | 200 |  | 6/21/85 | 8 | Forge Ingredient Crafting Level 2 | 1 GLASS_DEFAULT_BASE, 1 ITEM_BLOOD_VISCOUS, 1 ITEM_FRAGMENT_OLIVINE |
| FORGE_BASE_RESIN ×1 | EXTRACTOR | Standard | 0 | 30/300/1500 |  |  | 6/21/85 | 4 | Forge Ingredient Crafting Level 1 | 1 GLASS_DEFAULT_BASE, 1 ITEM_SAP_BASE, 1 ESSENCE_VITAL_ANCIENT |
| FORGE_COMPOUND_BOON_PURE1 ×4 | EXTRACTOR | Standard | 0 | 30/150/750 |  |  | 6/12/45 | 2 | Forge Ingredient Crafting Level 1 | 1 FORGE_BASE_COMPOUND, 2 ESSENCE_VITAL_FRESH |
| FORGE_COMPOUND_BOON_PURE2 ×4 | EXTRACTOR | Overdriven | 500 | 60/300/1500 | 200 |  | 7/21/85 | 8 | Forge Ingredient Crafting Level 2 | 2 FORGE_BASE_COMPOUND, 2 ESSENCE_VITAL_FRESH, 1 ITEM_ORB_PULSATING, 1 ITEM_EYE_SPITTER |
| FORGE_COMPOUND_BOON_PURE3 ×4 | EXTRACTOR | Overdriven | 2000 | 90/450/2250 | 400 |  | 9/29/100 | 13 | Forge Ingredient Crafting Level 3 | 4 FORGE_BASE_COMPOUND, 4 ESSENCE_VITAL_FRESH, 1 ITEM_ORB_SHIMMERING, 1 ITEM_EYE_SPITTER |
| FORGE_COMPOUND_BOON_IMPERFECT1 ×4 | EXTRACTOR | Standard | 0 | 30/150/750 |  |  | 6/12/45 | 2 | Forge Ingredient Crafting Level 1 | 1 FORGE_BASE_COMPOUND, 2 ESSENCE_VITAL_FRESH |
| FORGE_COMPOUND_BOON_IMPERFECT2 ×4 | EXTRACTOR | Overdriven | 500 | 60/300/1500 | 200 |  | 7/21/85 | 8 | Forge Ingredient Crafting Level 2 | 2 FORGE_BASE_COMPOUND, 2 ESSENCE_VITAL_FRESH, 1 ITEM_GLAND_ADRENAL, 1 ITEM_ORB_PULSATING |
| FORGE_COMPOUND_BOON_IMPERFECT3 ×4 | EXTRACTOR | Overdriven | 2000 | 90/450/2250 | 400 |  | 9/29/100 | 13 | Forge Ingredient Crafting Level 3 | 4 FORGE_BASE_COMPOUND, 4 ESSENCE_VITAL_FRESH, 2 ITEM_GLAND_ADRENAL, 1 ITEM_ORB_SHIMMERING |
| FORGE_COMPOUND_BOON_UNSTABLE1 ×4 | EXTRACTOR | Standard | 0 | 30/150/750 |  |  | 6/12/45 | 2 | Forge Ingredient Crafting Level 1 | 1 FORGE_BASE_COMPOUND, 2 ESSENCE_VITAL_FRESH |
| FORGE_COMPOUND_BOON_UNSTABLE2 ×4 | EXTRACTOR | Overdriven | 500 | 60/300/1500 | 200 |  | 7/21/85 | 8 | Forge Ingredient Crafting Level 2 | 2 FORGE_BASE_COMPOUND, 2 ESSENCE_VITAL_FRESH, 1 ITEM_EYE_WILDSTOCK, 1 ITEM_ORB_PULSATING |
| FORGE_COMPOUND_BOON_UNSTABLE3 ×4 | EXTRACTOR | Overdriven | 2000 | 90/450/2250 | 400 |  | 9/29/100 | 13 | Forge Ingredient Crafting Level 3 | 4 FORGE_BASE_COMPOUND, 4 ESSENCE_VITAL_FRESH, 2 ITEM_EYE_WILDSTOCK, 1 ITEM_ORB_SHIMMERING |
| FORGE_COMPOUND_BOON_DRAINING1 ×4 | EXTRACTOR | Overdriven | 1000 | 60/300/1500 | 200 |  | 7/21/85 | 8 | Forge Ingredient Crafting Level 2 | 2 FORGE_BASE_COMPOUND, 2 ESSENCE_VITAL_FRESH, 1 ITEM_EYE_WILDSTOCK, 1 ITEM_ORB_PULSATING |
| FORGE_COMPOUND_BOON_DRAINING2 ×4 | EXTRACTOR | Supercharged | 2500 | 90/450/2250 | 400 |  | 9/29/100 | 13 | Forge Ingredient Crafting Level 3 | 4 FORGE_BASE_COMPOUND, 4 ESSENCE_VITAL_FRESH, 2 ITEM_EYE_WILDSTOCK, 1 ITEM_ORB_SHIMMERING |
| FORGE_COMPOUND_BOON_CORRUPTED1 ×4 | EXTRACTOR | Overdriven | 1000 | 60/300/1500 | 200 |  | 7/21/85 | 8 | Forge Ingredient Crafting Level 2 | 2 FORGE_BASE_COMPOUND, 2 ESSENCE_VITAL_FRESH, 1 ITEM_EYE_HOPPER, 1 ITEM_ORB_PULSATING |
| FORGE_COMPOUND_BOON_CORRUPTED2 ×4 | EXTRACTOR | Supercharged | 2500 | 90/450/2250 | 400 |  | 9/29/100 | 13 | Forge Ingredient Crafting Level 3 | 4 FORGE_BASE_COMPOUND, 4 ESSENCE_VITAL_FRESH, 2 ITEM_EYE_HOPPER, 1 ITEM_ORB_SHIMMERING |
| FORGE_COMPOUND_REDUCTION_DEFECT1 ×4 | EXTRACTOR | Overdriven | 500 | 60/300/1500 | 100 |  | 7/21/85 | 7 | Forge Ingredient Crafting Level 1 | 2 FORGE_BASE_COMPOUND, 2 ITEM_FRAGMENT_OLIVINE, 1 ITEM_ORB_PULSATING |
| FORGE_COMPOUND_REDUCTION_DEFECT2 ×4 | EXTRACTOR | Overdriven | 1500 | 90/450/2250 | 300 |  | 9/29/100 | 13 | Forge Ingredient Crafting Level 2 | 4 FORGE_BASE_COMPOUND, 4 ITEM_FRAGMENT_OLIVINE, 1 ITEM_ORB_SHIMMERING |
| FORGE_COMPOUND_REDUCTION_DEFECT_UNSTABLE1 ×4 | EXTRACTOR | Overdriven | 500 | 60/300/1500 | 100 |  | 7/21/85 | 7 | Forge Ingredient Crafting Level 2 | 2 FORGE_BASE_COMPOUND, 2 ITEM_FRAGMENT_OLIVINE, 1 ITEM_GLAND_ADRENAL, 1 ITEM_ORB_PULSATING |
| FORGE_COMPOUND_REDUCTION_DEFECT_UNSTABLE2 ×4 | EXTRACTOR | Overdriven | 1500 | 90/450/2250 | 300 |  | 9/29/100 | 13 | Forge Ingredient Crafting Level 3 | 4 FORGE_BASE_COMPOUND, 4 ITEM_FRAGMENT_OLIVINE, 2 ITEM_GLAND_ADRENAL, 1 ITEM_ORB_SHIMMERING |
| FORGE_COMPOUND_REDUCTION_QUIRK1 ×4 | EXTRACTOR | Overdriven | 500 | 60/300/1500 | 100 |  | 7/21/85 | 7 | Forge Ingredient Crafting Level 2 | 2 FORGE_BASE_COMPOUND, 2 ITEM_FRAGMENT_OLIVINE, 1 ITEM_EYE_ROADRUNNER, 1 ITEM_ORB_PULSATING |
| FORGE_COMPOUND_REDUCTION_QUIRK2 ×4 | EXTRACTOR | Overdriven | 1500 | 90/450/2250 | 300 |  | 9/29/100 | 13 | Forge Ingredient Crafting Level 3 | 4 FORGE_BASE_COMPOUND, 4 ITEM_FRAGMENT_OLIVINE, 2 ITEM_EYE_ROADRUNNER, 1 ITEM_ORB_SHIMMERING |
| FORGE_COMPOUND_REDUCTION_QUIRK_IMPERFECT1 ×4 | EXTRACTOR | Overdriven | 500 | 60/300/1500 | 100 |  | 7/21/85 | 7 | Forge Ingredient Crafting Level 2 | 2 FORGE_BASE_COMPOUND, 2 ITEM_FRAGMENT_OLIVINE, 1 ITEM_EYE_SPITTER, 1 ITEM_ORB_PULSATING |
| FORGE_COMPOUND_REDUCTION_QUIRK_IMPERFECT2 ×4 | EXTRACTOR | Overdriven | 1500 | 90/450/2250 | 300 |  | 9/29/100 | 13 | Forge Ingredient Crafting Level 3 | 4 FORGE_BASE_COMPOUND, 4 ITEM_FRAGMENT_OLIVINE, 2 ITEM_EYE_SPITTER, 1 ITEM_ORB_SHIMMERING |
| FORGE_PASTE_PROTECTION1 ×4 | EXTRACTOR | Overdriven | 600 | 60/300/1500 | 200 |  | 7/21/85 | 8 | Forge Ingredient Crafting Level 2 | 2 FORGE_BASE_PASTE, 1 ITEM_MANTLE_CREATURE, 1 ITEM_ORB_PULSATING |
| FORGE_PASTE_PROTECTION2 ×4 | EXTRACTOR | Overdriven | 1800 | 90/450/2250 | 300 |  | 9/29/100 | 13 | Forge Ingredient Crafting Level 3 | 4 FORGE_BASE_PASTE, 2 ITEM_MANTLE_CREATURE, 1 ITEM_ORB_SHIMMERING |
| FORGE_PASTE_BOOST_BOON1 ×4 | EXTRACTOR | Overdriven | 600 | 60/300/1500 | 200 |  | 7/21/85 | 8 | Forge Ingredient Crafting Level 1 | 2 FORGE_BASE_PASTE, 2 ITEM_FRAGMENT_SALTPETER, 1 ITEM_ORB_PULSATING |
| FORGE_PASTE_BOOST_BOON2 ×4 | EXTRACTOR | Overdriven | 1800 | 90/450/2250 | 300 |  | 9/29/100 | 13 | Forge Ingredient Crafting Level 2 | 4 FORGE_BASE_PASTE, 4 ITEM_FRAGMENT_SALTPETER, 1 ITEM_ORB_SHIMMERING |
| FORGE_PASTE_GUARD_DEFECT1 ×4 | EXTRACTOR | Overdriven | 600 | 60/300/1500 | 200 |  | 7/21/85 | 8 | Forge Ingredient Crafting Level 1 | 2 FORGE_BASE_PASTE, 2 ITEM_HIDE_BASE, 1 ITEM_ORB_PULSATING |
| FORGE_PASTE_GUARD_DEFECT2 ×4 | EXTRACTOR | Overdriven | 1800 | 90/450/2250 | 300 |  | 9/29/100 | 13 | Forge Ingredient Crafting Level 2 | 4 FORGE_BASE_PASTE, 4 ITEM_HIDE_BASE, 1 ITEM_ORB_SHIMMERING |
| FORGE_PASTE_GUARD_QUIRK1 ×4 | EXTRACTOR | Overdriven | 600 | 60/300/1500 | 200 |  | 7/21/85 | 8 | Forge Ingredient Crafting Level 1 | 2 FORGE_BASE_PASTE, 2 ITEM_HIDE_BASE, 1 ITEM_ORB_PULSATING |
| FORGE_PASTE_GUARD_QUIRK2 ×4 | EXTRACTOR | Overdriven | 1800 | 90/450/2250 | 300 |  | 9/29/100 | 13 | Forge Ingredient Crafting Level 2 | 4 FORGE_BASE_PASTE, 4 ITEM_HIDE_BASE, 1 ITEM_ORB_SHIMMERING |
| FORGE_PASTE_ENVIGORATION1 ×4 | EXTRACTOR | Overdriven | 600 | 60/300/1500 | 200 |  | 7/21/85 | 8 | Forge Ingredient Crafting Level 2 | 2 FORGE_BASE_PASTE, 1 ITEM_ORB_VITAL, 1 ITEM_ORB_SHIMMERING |
| FORGE_PASTE_ENVIGORATION2 ×4 | EXTRACTOR | Overdriven | 2000 | 90/450/2250 | 400 |  | 9/29/100 | 13 | Forge Ingredient Crafting Level 3 | 4 FORGE_BASE_PASTE, 2 ITEM_ORB_VITAL, 1 ITEM_ORB_SHADOW |
| FORGE_PASTE_STABILISATION1 ×4 | EXTRACTOR | Overdriven | 600 | 60/300/1500 | 200 |  | 7/21/85 | 8 | Forge Ingredient Crafting Level 2 | 2 FORGE_BASE_PASTE, 2 ITEM_FRAGMENT_SULFUR, 1 ITEM_ORB_SHIMMERING |
| FORGE_PASTE_STABILISATION2 ×4 | EXTRACTOR | Overdriven | 2000 | 90/450/2250 | 400 |  | 9/29/100 | 13 | Forge Ingredient Crafting Level 3 | 4 FORGE_BASE_PASTE, 4 ITEM_FRAGMENT_SULFUR, 1 ITEM_ORB_SHADOW |
| FORGE_PASTE_FATE1 ×4 | EXTRACTOR | Overdriven | 600 | 30/150/750 | 200 |  | 6/12/45 | 3 | Forge Ingredient Crafting Level 2 | 1 FORGE_BASE_PASTE, 1 ITEM_GLAND_SPARKSAC, 1 ITEM_ORB_PULSATING |
| FORGE_PASTE_FATE2 ×4 | EXTRACTOR | Overdriven | 1800 | 60/300/1500 | 300 |  | 7/21/85 | 8 | Forge Ingredient Crafting Level 3 | 2 FORGE_BASE_PASTE, 2 ITEM_GLAND_SPARKSAC, 1 ITEM_ORB_SHIMMERING |
| FORGE_PASTE_REVERSAL_FATE ×4 | EXTRACTOR | Overdriven | 1500 | 90/450/2250 | 200 |  | 9/29/100 | 12 | Forge Ingredient Crafting Level 2 | 4 FORGE_BASE_PASTE, 4 ITEM_PETAL_BASE, 1 ITEM_ORB_SHIMMERING |
| FORGE_PASTE_GAMBLING1 ×4 | EXTRACTOR | Overdriven | 1000 | 30/150/750 | 100 |  | 6/12/45 | 3 | Forge Ingredient Crafting Level 2 | 2 FORGE_BASE_PASTE, 1 ITEM_ROCKDROP_OPAL, 1 ITEM_ORB_PULSATING |
| FORGE_PASTE_GAMBLING2 ×4 | EXTRACTOR | Overdriven | 2000 | 60/300/1500 | 200 |  | 7/21/85 | 8 | Forge Ingredient Crafting Level 3 | 4 FORGE_BASE_PASTE, 2 ITEM_ROCKDROP_OPAL, 1 ITEM_ORB_SHIMMERING |
| FORGE_GUM_EASE ×4 | EXTRACTOR | Standard | 0 | 30/150/750 |  |  | 6/12/45 | 2 | Forge Ingredient Crafting Level 1 | 1 FORGE_BASE_GUM, 1 ITEM_BLOOD_FRESH |
| FORGE_GUM_LIGHTNESS ×4 | EXTRACTOR | Standard | 0 | 30/150/750 |  |  | 6/12/45 | 2 | Forge Ingredient Crafting Level 1 | 1 FORGE_BASE_GUM, 1 ITEM_GLAND_FLOATATIONSAC |
| FORGE_GUM_SPECIAL ×4 | EXTRACTOR | Standard | 0 | 30/150/750 |  |  | 6/12/45 | 2 | Forge Ingredient Crafting Level 1 | 1 FORGE_BASE_GUM, 1 ITEM_BLOOD_VISCOUS |
| FORGE_GUM_EFFECT ×4 | EXTRACTOR | Standard | 0 | 30/150/750 |  |  | 6/12/45 | 2 | Forge Ingredient Crafting Level 1 | 1 FORGE_BASE_GUM, 1 ITEM_GLAND_THERMALSAC |
| FORGE_GUM_LONGEVITY ×4 | EXTRACTOR | Standard | 0 | 30/150/750 |  |  | 6/12/45 | 2 | Forge Ingredient Crafting Level 1 | 1 FORGE_BASE_GUM, 1 ITEM_MANTLE_CREATURE |
| FORGE_GUM_GLOW ×4 | EXTRACTOR | Standard | 0 | 30/150/750 |  |  | 6/12/45 | 2 | Forge Ingredient Crafting Level 1 | 1 FORGE_BASE_GUM, 1 ITEM_LAMELLA_GLOWING |
| FORGE_GUM_REDUCTION_TRAIT ×4 | EXTRACTOR | Standard | 0 | 30/150/750 |  |  | 6/12/45 | 2 | Forge Ingredient Crafting Level 1 | 2 FORGE_BASE_GUM, 1 ITEM_FRAGMENT_OLIVINE |
| FORGE_GUM_EXPANSION_TRAIT ×4 | EXTRACTOR | Standard | 0 | 60/300/1500 |  |  | 7/21/85 | 7 | Forge Ingredient Crafting Level 1 | 2 FORGE_BASE_GUM, 1 ITEM_FRAGMENT_SULFUR |
| FORGE_CATALYST_REVERSAL_DEFECT ×4 | EXTRACTOR | Supercharged | 2500 | 90/450/2250 | 500 |  | 9/29/100 | 14 | Forge Ingredient Crafting Level 3 | 4 FORGE_BASE_CATALYST, 1 ITEM_TROPHY_CUTTLETRUNK, 1 ITEM_ORB_SHIMMERING, 1 ITEM_ORB_SHADOW |
| FORGE_CATALYST_VIGOUR1 ×4 | EXTRACTOR | Overdriven | 1500 | 60/300/1500 | 200 |  | 7/21/85 | 8 | Forge Ingredient Crafting Level 2 | 4 FORGE_BASE_CATALYST, 1 ITEM_TROPHY_HOPPER, 1 ITEM_ORB_SHIMMERING |
| FORGE_CATALYST_VIGOUR2 ×4 | EXTRACTOR | Supercharged | 3000 | 90/450/2250 | 500 |  | 9/29/100 | 14 | Forge Ingredient Crafting Level 3 | 4 FORGE_BASE_CATALYST, 1 ITEM_TROPHY_HOPPER, 1 ITEM_ORB_SHIMMERING, 1 ITEM_ORB_SHADOW |
| FORGE_CATALYST_BOOST_BOON ×4 | EXTRACTOR | Supercharged | 2500 | 90/450/2250 | 500 |  | 9/29/100 | 14 | Forge Ingredient Crafting Level 3 | 3 FORGE_BASE_CATALYST, 1 ITEM_TROPHY_SPITTER, 1 ITEM_ORB_SHIMMERING |
| FORGE_CATALYST_POINTS_BOON1 ×4 | EXTRACTOR | Overdriven | 1500 | 60/300/1500 | 200 |  | 7/21/85 | 8 | Forge Ingredient Crafting Level 2 | 2 FORGE_BASE_CATALYST, 1 ITEM_TROPHY_SPITTER, 1 ITEM_ORB_SHIMMERING |
| FORGE_CATALYST_POINTS_BOON2 ×4 | EXTRACTOR | Supercharged | 3000 | 90/450/2250 | 500 |  | 9/29/100 | 14 | Forge Ingredient Crafting Level 3 | 4 FORGE_BASE_CATALYST, 1 ITEM_TROPHY_SPITTER, 1 ITEM_ORB_SHIMMERING, 1 ITEM_ORB_SHADOW |
| FORGE_SOLVENT_REMOVAL_BOON1 ×4 | EXTRACTOR | Powered | 0 | 60/300/1500 | 200 |  | 7/21/85 | 8 | Forge Ingredient Crafting Level 1 | 2 FORGE_BASE_SOLVENT, 1 ITEM_TROPHY_WILDSTOCK, 1 ITEM_ORB_SHIMMERING |
| FORGE_SOLVENT_REMOVAL_BOON2 ×4 | EXTRACTOR | Overdriven | 1200 | 90/900/4500 | 400 |  | 9/55/120 | 11 | Forge Ingredient Crafting Level 2 | 4 FORGE_BASE_SOLVENT, 1 ITEM_TROPHY_WILDSTOCK, 1 ITEM_ORB_SHIMMERING, 1 ITEM_ORB_SHADOW |
| FORGE_SOLVENT_REMOVAL_DEFECT1 ×4 | EXTRACTOR | Overdriven | 1200 | 60/300/1500 | 400 |  | 7/21/85 | 9 | Forge Ingredient Crafting Level 2 | 3 FORGE_BASE_SOLVENT, 1 ITEM_TROPHY_WILDSTOCK, 1 ITEM_ORB_SHIMMERING |
| FORGE_SOLVENT_REMOVAL_DEFECT2 ×4 | EXTRACTOR | Supercharged | 2600 | 90/450/2250 | 600 |  | 9/29/100 | 14 | Forge Ingredient Crafting Level 3 | 4 FORGE_BASE_SOLVENT, 1 ITEM_TROPHY_ROADRUNNER, 1 ITEM_ORB_SHIMMERING, 1 ITEM_ORB_SHADOW |
| FORGE_SOLVENT_REMOVAL_QUIRK1 ×4 | EXTRACTOR | Overdriven | 1200 | 60/300/1500 | 400 |  | 7/21/85 | 9 | Forge Ingredient Crafting Level 2 | 3 FORGE_BASE_SOLVENT, 1 ITEM_TROPHY_WILDSTOCK, 1 ITEM_ORB_SHIMMERING |
| FORGE_SOLVENT_REMOVAL_QUIRK2 ×4 | EXTRACTOR | Supercharged | 2600 | 90/450/2250 | 600 |  | 9/29/100 | 14 | Forge Ingredient Crafting Level 3 | 4 FORGE_BASE_SOLVENT, 1 ITEM_TROPHY_ROADRUNNER, 1 ITEM_ORB_SHIMMERING, 1 ITEM_ORB_SHADOW |
| FORGE_SOLVENT_TRANSMUTE_BOON ×4 | EXTRACTOR | Supercharged | 2600 | 90/450/2250 | 600 |  | 9/29/100 | 14 | Forge Ingredient Crafting Level 3 | 4 FORGE_BASE_SOLVENT, 1 ITEM_TROPHY_ROADRUNNER, 1 ITEM_ORB_SHIMMERING, 1 ITEM_ORB_SHADOW |
| FORGE_SOLVENT_TRANSMUTE_DEFECT ×4 | EXTRACTOR | Powered | 0 | 90/450/2250 | 200 |  | 9/29/100 | 12 | Forge Ingredient Crafting Level 1 | 2 FORGE_BASE_SOLVENT, 1 ITEM_TROPHY_WILDSTOCK, 1 ITEM_ORB_SHIMMERING |
| FORGE_SOLVENT_TRANSMUTE_QUIRK ×4 | EXTRACTOR | Powered | 0 | 90/450/2250 | 200 |  | 9/29/100 | 12 | Forge Ingredient Crafting Level 1 | 2 FORGE_BASE_SOLVENT, 1 ITEM_TROPHY_WILDSTOCK, 1 ITEM_ORB_SHIMMERING |
| FORGE_RESIN_SETTING ×4 | EXTRACTOR | Standard | 0 | 30/150/750 |  |  | 6/12/45 | 2 | Forge Ingredient Crafting Level 1 | 1 FORGE_BASE_RESIN, 1 ITEM_ROCKDROP_FLINT |
| FORGE_RESIN_DECONSTRUCTION1 ×4 | EXTRACTOR | Standard | 0 | 30/150/750 |  |  | 6/12/45 | 2 | Forge Ingredient Crafting Level 1 | 1 FORGE_BASE_RESIN, 1 ITEM_BLOOD_VOLATILE |
| FORGE_RESIN_DECONSTRUCTION2 ×4 | EXTRACTOR | Overdriven | 1000 | 60/300/1500 | 300 |  | 7/21/85 | 8 | Forge Ingredient Crafting Level 2 | 2 FORGE_BASE_RESIN, 2 ITEM_BLOOD_VOLATILE, 1 ITEM_ORB_PULSATING |
| FORGE_RESIN_DECONSTRUCTION3 ×4 | EXTRACTOR | Supercharged | 3000 | 90/450/2250 | 500 |  | 9/29/100 | 14 | Forge Ingredient Crafting Level 3 | 4 FORGE_BASE_RESIN, 4 ITEM_BLOOD_VOLATILE, 1 ITEM_ORB_SHIMMERING, 1 ITEM_ORB_SHADOW |
| FORGE_GUM_VENERABLE ×4 | EXTRACTOR | Standard | 0 | 30/150/750 |  |  | 6/12/45 | 2 | Forge Ingredient Crafting Level 1 | 1 FORGE_BASE_GUM, 20 FOOD_EARTHYAM_BASE_RAW |

## 3. Gear stats ([BI-A] `item-attributes.json`)

### Spanners (group "Spanners")

| Material | Rank | Action speed | Durability | Energy (Burst Stamina Drain) | Flex | Wear removed per hit | Element |
|---|---|---|---|---|---|---|---|
| Stone | 20 | 40 | 600 | 4 | 120 | 100 | |
| Copper | 30 | 70 | 500 | 6 | 110 | 400 | |
| Iron | 30 | 30 | 900 | 8 | 110 | 700 | |
| Silver alloy | 50 | 70 | 1100 | 10 | 100 | 1500 | |
| Gold alloy | 50 | 40 | 1000 | 15 | 90 | 2000 | |
| Titanium alloy | 80 | 50 | 1500 | 30 | 90 | 6000 | |
| Amethyst | 100 | 50 | 2000 | 50 | 70 | 10000 | Corrosion |
| Diamond | 100 | 50 | 2000 | 50 | 80 | 10000 | Blast |
| Emerald | 100 | 50 | 2000 | 50 | 80 | 10000 | Toxin |
| Topaz | 100 | 50 | 2000 | 50 | 70 | 10000 | Shock |
| Ruby | 100 | 50 | 2000 | 50 | 75 | 10000 | Burn |
| Sapphire | 100 | 50 | 2000 | 50 | 75 | 10000 | Chill |

- **Own-machine bonus for gem spanners: not in the data.** Each gem spanner has its own attribute bundle (`Item Diamond Spanner Initial`, ... [MAY-I]) but the resolved values in [BI-A] are identical for all six and list nothing per machine. The wiki's "most effective" claim stays unconfirmed.
- [MAY-I] also has `repairPointsOnUse` per spanner: stone 0.01, copper 0.02, iron 0.05, silver 0.1, gold 0.2, titanium 0.3, every gem 0.3. Meaning unknown (not the wear removed; perhaps XP or a share); gem = titanium here.
- Hunger drain per use = energy / 100, as for tools.

### Chisels (group "Chisels"): only two stat sets

| Class | Who | Rank | Action speed | Durability | Energy | Hunger drain | Can damage hardness | Flex |
|---|---|---|---|---|---|---|---|---|
| Basic | stone, copper, iron, silver, gold, titanium | 50 | 50 | 800 | 10 | 0.2 | 3 | 100 |
| Advanced | the six gems, rift, umbris, blink, oortstone | 100 | 80 | 2000 | 20 | 0.4 | 10 | 100 |

Cut per chisel ([MAY-I] `chiselType` / `chiselPrecision`; type 1 = square, 2 = bevel, 0 = the copper and silver chisels' cut, read as slope; precision 1 = precise, 2 = the Lucent "finest"): stone 1/1, copper 0/1, iron 2/1, silver 0/0, gold 2/0, titanium 1/0, amethyst 1/1, diamond 2/0, emerald 2/1, sapphire 1/0, **ruby 0/1**, **topaz 0/0**, rift 2/2, umbris 1/2, blink 0/2, oortstone 1/0.

### Gem tools (group "Tools"): tuning 2.1 confirmed exactly

Every row of tuning 2.1 matches [BI-A] (damage, speed, crit chance and effect, durability, energy, hardness 10, Flex, rank), including silver, gold, titanium and all six gems. Tools in [BI-A] list no element; spanners do (above), and the gem's element on tools comes from the weapon data (tuning 3.3.3). Grapples (group "Grapples") match tuning 4.9 exactly, with hunger drain = energy / 500.

## 4. Block drops and mining XP ([PF-D] expected count per block by tier, no luck, no gathering epic; [PF-S] `mineXP`)

| Block (data name) | Drop | T1 | T2 | T3 | T4 | T5 | T6 | T7 | T8 | Mine XP |
|---|---|---|---|---|---|---|---|---|---|---|
| Copper seam | copper ore | 1.5 | 1.5 | 1.7 | 1.7 | 2.05 | 2.05 | 2.05 | 2.05 | 24 |
| Iron seam | iron ore | 1.5 | 1.5 | 1.7 | 1.7 | 2.05 | 2.05 | 2.05 | 2.05 | 40 |
| Silver seam | silver ore | 1.0 | 1.0 | 1.0 | 1.5 | 1.5 | 1.7 | 1.7 | 2.05 | 64 |
| Gold seam | gold ore | 1.0 | 1.0 | 1.0 | 1.5 | 1.5 | 1.7 | 1.7 | 2.05 | 80 |
| Titanium seam | titanium ore | 1.0 | 1.0 | 1.0 | 1.0 | 1.5 | 1.8 | 1.8 | 2.2 | 136 |
| Soft coal seam | soft coal | 1.5 | 1.5 | 1.7 | 1.7 | 2.05 | 2.05 | 2.05 | 2.05 | 24 |
| Coal seam (medium) | medium coal | 1.0 | 1.0 | 1.5 | 1.5 | 1.7 | 1.7 | 2.05 | 2.05 | 64 |
| Hard coal seam | hard coal | 1.0 | 1.0 | 1.0 | 1.0 | 1.5 | 1.8 | 1.8 | 2.2 | 104 |
| Each gem seam (6) | rough gem | 1.0 | 1.0 | 1.0 | 1.0 | 1.5 | 1.8 | 1.8 | 2.2 | 176 |
| Ancient tech remnant seam | remnant | 1.5 | 1.5 | 1.7 | 1.7 | 2.05 | 2.05 | 2.05 | 2.05 | 24 |
| Ancient tech component seam | component | 1.0 | 1.0 | 1.5 | 1.5 | 1.7 | 1.7 | 2.05 | 2.05 | 64 |
| Ancient tech device seam | device (relic core) | 1.0 | 1.0 | 1.0 | 1.0 | 1.5 | 1.8 | 1.8 | 2.2 | 104 |
| Small / medium / large fossil seam | fossil | as remnant / component / device | | | | | | | | 24 / 64 / 104 |
| Each trunk (3 woods) | itself | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 2 |
| | sap | 0.08 | 0.08 | 0.08 | 0.08 | 0.12 | 0.12 | 0.12 | 0.12 | |
| | bark | 0 | 0 | 0.03 | 0.03 | 0.03 | 0.04 | 0.04 | 0.04 | |

- Structure: three yield ladders by rarity (common 1.5→2.05, uncommon 1.0→2.05, rare 1.0→2.2), each a sum of independent rolls per tier ([PF-D] raw entries). Boundmore's `bonus` tables already equal these (expected count − 1).
- Mining XP follows the same three steps (24 / 64 / 104) with metals and gems above: iron 40, gold 80, titanium 136, gems 176. Every seam has `buildXP` 2 ([PF-S]; Boundmore has 0).
- Fuel ([MAY-I] `fuelData`: spark, seconds, generator wear): soft coal 100 / 10 / 0.001, medium 250 / 20 / 0.003, hard 800 / 30 / 0.004; compact 1500 / 150 / 0.01, 4500 / 300 / 0.02, 15000 / 500 / 0.04; enriched 1650 / 75, 4950 / 150, 16500 / 250 (tuning 4.7 confirmed; wear is new).

## 5. Atmosphere, lava, elements, power

| Question | What the data says | Source | Conf. |
|---|---|---|---|
| Atmosphere damage | an item `DAMAGE_ATMOSPHERE` exists (the damage source, sound category `atmosphereDamage`); no numbers | [MAY-I] | high that it exists |
| Protection skill text | "Increases Caustic Atmosphere Protection by 1/1/1/2/2 which increases **the time you can spend** in Caustic Atmospheres **before taking Damage**" (same for Potent, Volatile) | [BLX] `/skills/` | high. Read with tuning 4.13.1: protection below the hazard does not stop the clock, it lengthens it, so a smaller shortfall gives more time. Supports making the air bar's drain depend on the shortfall |
| Atmosphere food per line | yes (headline 1) | [MAY-R], [MAY-I] | high |
| Lava | `PLACEABLE_LAVA` is a liquid item (`liquidType` 32, category `LIQUID`); a guild buff machine `MACHINE_GUILD_BUFF_LAVA_PROTECTION` exists (workbench, 3,600 s, 1,000 spark, 16 refined iron, 20 remnants, 16 volatile blood, 10 stones; ×5). No damage number | [MAY-I], [MAY-R] | high that lava protection is a guild buff, not a ward |
| Elemental creature modifiers | health +100%, damage +50%, armour +50%, drops +100%, XP +50%; "Special" world creatures +200% / +100% / +100% / +200% / +75% | [PF-C] `aWorldMods` (columns `hp,dmg,def,drop,xp`) | high (pfiffel built these from data) |
| Elemental hit chance | a matching element doubles damage with 30% chance (`chanceElem = 0.3`); fists 0 | [PF-C] `damageCalculator.js` | high; already tuning 3.3 |
| Elemental spawn chance | not in any file | | |
| Power above the need | no formula or constants in the recipe or item files | | |
| Coil power, wear, durability | none in [MAY-I]: coil items differ from machines only in id, palette and category (`ENGINE` / `ADVANCED_ENGINE`) | [MAY-I] | |
| Forge momentum constants | `momentumBase` 0.2, `momentumHigh` 1, `momentumMaxValue` 2.25, `momentumPowerCapBase` 1000, `momentumPowerCapHigh` 14500, **`momentumPowerCapCurvePower` 2**, intelligence cap 30 to 135 (curve 1), flexibility proportion 0.5 to 5 (curve 2), `flexibilityMaxValue` 150 | [MAY-F] `data` | high; M17 used these but not the power cap's curve 2 |

## Not in the data

- The atmosphere air bar's length, the damage once it runs out, the tick, and how the shortfall scales either (the skill text only says protection lengthens the time).
- The atmosphere food buff's points and length (bundle values are not exported; tuning 3.9's +1/+2/+3, 30 min stands).
- Lava damage per second.
- Elemental creature spawn chance per world (M18's 30% stays a guess; the only 0.3 in [PF-C] is the weapon's element hit chance).
- Power above the need: the time and spark formula, and the thresholds behind the level labels (the labels follow the recipe; see 1).
- Coil power (100 / 300 stays the wiki's), coil wear capacity and wear per craft.
- Machine wear capacity.
- Whether a gem spanner repairs more on its own machine (both [BI-A] and [MAY-I] show identical gem spanners).
- What `repairPointsOnUse` measures.
- `knowledgeUnlockLevel` per recipe is in [MAY-R] (gem gear 20, coils 30, compact coal 30) but its meaning (character level? knowledge page?) is not stated.

## Upstream fixes this suggests (not made here)

Atmosphere food per line and its real recipes (needs elemental shards and amalgams); purified buckets need no power; coil inputs fixed per coil; topaz and sapphire grapples add a relic component; titanium gear at crafting level 2; real XP and wear on every M18 recipe; spanner stats; mining XP; ruby and topaz chisel cuts swapped; chisel Flex 100; large fossils (10 essence at 500 power) and bark from nuts (500 power).
