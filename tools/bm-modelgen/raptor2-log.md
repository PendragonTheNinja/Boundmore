# raptor2: render, critique, fix

Each round renders front, three-quarter, side, back and a head close-up
(`raptor2.py OUT --preview` for shape rounds; without `--preview` it builds the
game model, bakes and renders that).

1. **First cage.** Torso rings, arm and leg holes, knuckles split into
   fingers, toes from the foot box, a neck entering under the head. Clean
   surfaces, but a mannequin: a tube torso, thin arms, featureless head, crest
   of wide flat cards, a pale belly over the whole front, and yellow wrist
   bands (the integer part id was being averaged by the subdivision).
2. **Masses and features.** Part ids per face (no blending); broader deltoids,
   a deep chest and narrow waist, thicker thighs and drumsticks; eyes, a
   two-part beak with crisp cutting edges, a creased brow and mouth line; belt,
   brass buckle, loincloth front and back, hip pouch. Now a bird character,
   but a round parrot head with a big beak, tiny eyes, a thick upright neck.
3. **The raptor head.** A long, low, square-sectioned skull; the beak only at
   the snout's end and hooked; larger eyes set forward; separate creased brow
   plates swept up and back (the scowl); narrower, longer crest blades and
   wider side blades; a ruff reaching round to the front; a slimmer neck;
   chunkier forearms; knees and feet set wider. Reads as a raptor in profile.
4. **The full pipeline.** Game copy (cage subdivided once, rigged to the
   kit's skeleton: body by automatic weights, accessories riding a bone or
   taking the body's weights where they sit), smart UVs, a bake from the
   copy subdivided three times: painted colour, detail (mottling, spots,
   scales), pointiness, facing-up and height, ambient occlusion, tint mask,
   normals; composed into the painted texture. 18,160 triangles: over budget,
   feathers and talons now stay unsubdivided (faceted) in the game copy.
5. **Budget and files.** Feathers and talons faceted (unsubdivided) in the
   game copy: 11,104 triangles. Painting cached per vertex and written in bulk
   (from tens of minutes to 2 s; the whole build now takes under two
   minutes). Textures baked at 2048, saved at 1024; the GLB no longer embeds
   them (8.4 MB to 0.6 MB). Markings softened: hard white tape before.
6. **Silhouette and colour.** Side crest blades splayed so the front view has
   a crown; brighter, bluer slate, a warm cream belly, paler feathers;
   stronger painted top light and occlusion; arms bent a little at the elbow
   with heavier forearms; a fuller drumstick; a narrower, longer loincloth.
7. **A bigger head.** Everything on the head grows 1.12 about the skull; the
   mouth line is painted dark along its crease; feathers run deep blue at the
   root to pale at the tip.
8. **Chest and edges.** Pectoral masses and a sternum groove in the torso
   rings; the edge highlight only on true edges (it drew pale seams down the
   thighs); front ruff feathers shorter (they lay like lapels); side crest
   blades swept back instead of out like ears; the head close-up framed.
9. **Clean-up and motion.** Inner-thigh and back markings removed (they read
   as streaks and scratches); a walk frame checked: legs, arms, belt and flaps
   deform without tearing. Exported GLB checked: 17 bones, idle and walk,
   every vertex's weights summing to one.

Still short of the goal (for Nathan to judge): the anatomy is simple (few
muscle planes beyond the chest; the thighs still read a little like
trousers); the painting is procedural, not brushwork, so it lacks the
hand-placed highlights of a real painted texture; no face rig or blinking.
