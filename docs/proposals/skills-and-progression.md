# Proposal: skills and progression

*Written 2026-10-20 for Nathan to decide. Nothing here is built.*

## Decided (Nathan, 2026-10-20)

"I fully accept it all as long as it is completely following how Boundless did it." So every answer below is **exactly Boundless's live game (1.0 to Release 248)**, which overrides the recommendations that differed:

- **Pages**: exactly Boundless. Up to 100 points each; a switch has a 15-minute cooldown unless you are at home; extra pages are bought with the currency Boundless gave per level (cubits, about 100 to 300 a level). That currency and the place to spend it (Boundless's Exchange) get built with the skills; not free pages.
- **Respec**: free until level 20, one cleanse point per level, and resets bought with the same currency, as in Boundless.
- **Death**: Boundless's XP debt (10% of the next level, half XP until repaid), when combat exists.
- **Base movement and reach**: Boundless's untrained values (run 3.0, sprint 5.0, reach 4), raised by Agility and Control (Nathan's separate answer, the same day).
- **Order of building**: as question 7 recommends (mining-related skills first, the rest with their systems).
- **Names** stay Boundmore's own (CLAUDE.md: never Boundless's names or text); meanings and numbers are Boundless's.

*The original proposal follows. The numbers come from `docs/research/boundless-tuning.md` section 5 (levels, skills, XP), section 2.7 (skills that change mining) and section 1.6 (movement); player opinions from `docs/research/boundless-forum-feedback.md` (section 9 and principle 5).*

Progression is what players invest in for years, so the CLAUDE.md rule applies: build nothing here until you have decided. Each question below has a recommendation; answering "yes to all" is a complete answer.

## What Boundless did (live game, 1.0 to Release 248)

- **Levels.** XP from mining, building, crafting, gathering, discovering regions and fighting. 500 XP to reach level 2, rising in steps to 200,000 per level at 50 (3.3 million XP in all). Past 50 you keep levelling at 200,000 a level.
- **Skill points.** 4 at level 2, then 2 a level: 100 by level 50. After 50, 4 a level, used only to fill extra skill pages.
- **Skill pages.** Each page holds up to 100 points. Characters had 3 pages at launch, 5 by 2020, extra ones bought. Switching costs a 15-minute cooldown, or is free at home. Points are one pool shared by every page.
- **Respec.** Free until level 20. After that, one "cleanse" point per level (each removes one spent point), or bought resets.
- **Categories unlock by level:** attributes at 1, mastery 2, crafting 4, epic 1 at 6, endurance 8, epic 2 at 10, exploration 12, epic 3 at 14.
- **Attributes** (8 ranks each, 1 point a rank, plus a 5-rank bonus to all of them):
  - **Power**: +5% damage a point.
  - **Dexterity**: +2 action speed a point.
  - **Agility**: run 3.0 to 5.7 m/s, sprint 5.0 to 9.8 m/s, and about +2.2% crit chance a point.
  - **Control**: reach 4 m + 1 a point.
  - **Luck**: more drops.
  - **Vitality**: health.
  - **Zeal**: energy, and buff durations.
  - **Intelligence**: healing, and forging.
- **Mastery** (5 ranks): +5% damage and +2 action speed with one tool type.
- **Epics** (5 points each, some either/or):
  - Damage: triples Power's share.
  - Hammer, or shovel and axe: +25%.
  - Light source: a personal light.
  - Chisels: precise cuts.
  - Plus durability drain, gathering, double jump, high jump, portal, and others.
- **Crafting skills** gate recipes by level (tool crafting 1 for copper and iron tools, 2 for silver to titanium, and so on).
- **Death**: an XP debt of 10% of the next level. You earn half XP until it is repaid.

Players' main complaints (forum research, section 9): points ran out, so one character could not do everything; alts were the efficient choice; specialising got boring after level 20. The death XP debt made people log off.

## Questions

### 1. Which Boundless?
The live tree (1.0 to Release 248) is what players knew. Testing 249's rework (2 points a level to 180, separate points per page, melee skills) never went live.
**Recommendation: the live tree.** It is the one Boundless players will expect, and the data for it is complete.

### 2. Levels, XP and points: copy exactly?
Same curve, same points per level, same XP per action. Mining XP: 2 per natural block, 24 copper, 40 iron, and so on. Building XP: only for blocks placed inside your own claim. Crafting XP comes per recipe; discovering regions gives 50, then 100, 150 and up.
**Recommendation: yes, exactly.** We already have the per-block and per-recipe data.

### 3. "One character can learn everything"?
The forum research's principle 5 says one character should eventually be able to do everything. Boundless's pages already allow it in the long run: after 50, every level adds 4 points for more pages. The 100-point cap per page is what forces choices at any one moment.
Options:
- **(a)** Exactly Boundless: 100 a page, 5 pages, a 15-minute switch away from home.
- **(b)** The same, with free page switching anywhere.
- **(c)** No cap: one big page.

**Recommendation: (a), but with pages free (Boundless sold extra ones).** It keeps Boundless's builds-and-roles feel, and long play still unlocks everything.

### 4. Respec
**Recommendation: exactly Boundless.** Free until level 20, then one cleanse point per level. A bought reset becomes a craftable or earnable item, since we have no shop.

### 5. Death
Boundless's 10% XP debt is the death rule players complained about most.
**Recommendation: decide when combat is built**, since nothing can kill you yet. Lean towards a lighter penalty (forum principle 6).

### 6. Base movement and reach
Today everyone walks at 4.6 and sprints at 7.6 blocks a second, and reaches 7 blocks. Boundless's untrained character runs at 3.0, sprints at 5.0 and reaches 4 m; skills take that to 5.7, 9.8 and 12 m.
Matching Boundless makes a new character noticeably slower and shorter-armed than today, and a trained one faster and longer-reaching.
**Recommendation: match Boundless** (you said Boundless first). But this is the change you will feel most, so it is your call. You know what an untrained Boundless character felt like.

### 7. Order of building
Systems that do not exist yet (health, energy, creatures, forging, atmosphere) make half the tree meaningless today.
**Recommendation: two steps.**
- **Now:** levels and XP; points and pages; Power, Dexterity, Agility, Control and Luck; tool masteries; the mining epics (damage, hammer, shovel and axe, durability drain), the light-source epic and the chisels epic. Precise chisels move from a better item to the skill, as in Boundless; existing precise chisels stay usable.
- **Later**, each with its system: Vitality and Zeal with health and energy; endurance and exploration with combat and hazards; crafting levels with the crafting chain; the forging epic with forging.

### 8. Names
Skills get Boundmore's own names and text (the rules of the spirit). **Recommendation:** plain names close in meaning (Strength for Power, Quickness for Dexterity, and so on); I propose a full list in the spec.

## What it would cost to change later

Levels, XP and spent points are saved per account and shape everything players do. Changing the curve or point counts after people have played means migrating saves and redoing balance. That is why this waits for your answer. Effects (the +5% a point) are data and cheap to retune.
