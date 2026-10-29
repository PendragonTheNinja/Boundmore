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
