#714 live Windows disclosure re-verify (closed-height + sibling reflow)
tip: cfd7587c5c16f13250d19322275dfdb756952c01
out: C:\Users\oded\src\ai-buddy\.verify\714-disclosure-20260915-042259

## Presence
- check1 full label: PASS
- check2 no reserved band: PASS
- check3 toggle: PASS
- check3 reflow: rows below moved by 43 px on expand (y 50 -> 93)
- check3 baseline: collapse returned baseline y=50 (backDelta=0)
- disclosures: 2
- gaps: btn@y=22 gapToNext=4 next='Stays on screen, silences sounds, stops initiating actions.' hiddenStaticH=39; btn@y=108 gapToNext=4 next='Off silences audio cues.' hiddenStaticH=26
- screenshots: Presence-closed.png, Presence-open.png, Presence-reclosed.png

## AI
- check1 full label: PASS
- check2 no reserved band: PASS
- check3 toggle: PASS
- check3 reflow: rows below moved by 43 px on expand (y 54 -> 97)
- check3 baseline: collapse returned baseline y=54 (backDelta=0)
- disclosures: 8
- gaps: btn@y=26 gapToNext=4 next='AI on' hiddenStaticH=39; btn@y=90 gapToNext=4 next='Lets the model pick what happens next.' hiddenStaticH=52; btn@y=176 gapToNext=4 next='Acts on its own, not only when asked.' hiddenStaticH=39; btn@y=370 gapToNext=4 next='AI source' hiddenStaticH=65; btn@y=456 gapToNext=4 next='Which "AI brain" answers for the buddy.' hiddenStaticH=39; btn@y=564 gapToNext=4 next='' hiddenStaticH=18; btn@y=704 gapToNext=4 next='Harness signs itself in - ai-buddy never asks for credentials.' hiddenStaticH=39; btn@y=1284 gapToNext=4 next='The last thing sent to the model.' hiddenStaticH=26
- screenshots: AI-closed.png, AI-open.png, AI-reclosed.png

