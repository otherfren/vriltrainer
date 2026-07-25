# Feature Specification: Remote Viewing Trainer

**Feature Branch**: `001-remote-viewing-trainer`

**Created**: 2026-07-25

**Status**: Draft

**Input**: User description: "Online trainer for remote viewing. A user invents a name and receives a secret access link. Trial loop: a coordinate is shown, a click reveals 8 images, the user picks one, the next click shows right or wrong plus a cryptographic proof, the next click starts over. Score tracking, leaderboard, statistics with z-score from 10 trials. Bilingual across two domains, public audit log."

**Design basis**: Mechanism and concept decisions were settled in two prior interviews and are
recorded in `docs/trial-protocol-decisions.md` (D1–D21). That document is binding on planning;
this specification states what the product must do, not how.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Complete a viewing trial (Priority: P1) 🎯 MVP

Someone curious about remote viewing arrives at the site, invents a name, and is immediately
running trials. A coordinate appears. They sit with it, form an impression, then reveal eight
candidate images and pick the one matching their impression. The system tells them whether they
were right and offers the next trial.

**Why this priority**: Without the trial loop there is no product. Everything else — scoring,
statistics, verification — describes trials that must first exist.

**Independent Test**: Enter a name in a fresh browser, run one trial from coordinate to verdict,
and start a second. Delivers the core practice experience with no other feature present.

**Acceptance Scenarios**:

1. **Given** a first-time visitor, **When** they enter a self-chosen name, **Then** they reach a
   coordinate without any registration, email, or password step.
2. **Given** a coordinate is displayed, **When** the visitor acts to reveal, **Then** exactly
   eight candidate images appear, exactly one of which is the target.
3. **Given** eight images are displayed, **When** the visitor selects one, **Then** the system
   reports whether the selection matched the target.
4. **Given** a completed trial, **When** the visitor acts again, **Then** a new trial begins with
   a new coordinate.
5. **Given** a new account, **When** it is created, **Then** the visitor receives a personal
   secret access link that restores the account in any browser.
6. **Given** the access link is on screen, **When** the visitor has not explicitly revealed it,
   **Then** it is displayed masked and can still be copied without being revealed.

---

### User Story 2 - Find out whether I am beating chance (Priority: P2)

A returning practitioner has run trials across several sessions and wants to know whether their
results mean anything. They open their statistics and see how far their hit rate sits from what
chance would produce, and what that distance is worth.

**Why this priority**: This is the distinguishing feature. A trainer that only says right or
wrong is a guessing game; the claim that results can be evaluated is what makes the product
worth using.

**Independent Test**: Run trials until the threshold is reached, open the statistics view, and
confirm the deviation from chance is reported together with the context needed to interpret it.

**Acceptance Scenarios**:

1. **Given** a user with fewer than ten completed trials, **When** they look for statistics,
   **Then** the view is withheld and the remaining trial count is stated.
2. **Given** a user with ten completed trials and no hits at all, **When** they open statistics,
   **Then** the view is shown — the threshold depends on trial count alone, never on success.
3. **Given** a user's statistics are displayed, **When** the deviation from chance is shown,
   **Then** it is accompanied by how many users would reach that value by chance alone.
4. **Given** a user completes an individual trial, **When** their statistics update, **Then** the
   reported figure advances in blocks of completed trials rather than after every single one.
5. **Given** any visitor, **When** they view the statistics page, **Then** an aggregate result
   across all trials by all users is presented, including users who never reached the threshold.
6. **Given** a user who has abandoned trials, **When** their statistics are shown, **Then** the
   number of abandoned trials is shown alongside, so selective abandonment is visible.

---

### User Story 3 - Satisfy myself that the game is not rigged (Priority: P3)

A sceptical visitor sees a leaderboard claiming significant results. They want to know whether
the operator could simply be deciding outcomes after the fact. They check an individual trial's
evidence in the interface, then download the complete record and recompute it themselves.

**Why this priority**: The significance claim is worth only as much as its verifiability. This
story is what separates the product from an unfalsifiable psi site, but trials must exist and be
scored before there is anything to verify.

**Independent Test**: Complete a trial, inspect its evidence in the interface, then fetch the
public record and confirm independently that the outcome was fixed before the choice. The
verification half of this story stands alone; **the leaderboard half does not** — it ranks the
statistics built in User Story 2 and cannot be delivered before them.

**Acceptance Scenarios**:

1. **Given** a completed trial, **When** the outcome is revealed, **Then** the user is shown
   evidence they can check that the target was fixed before their choice and corresponds to the
   coordinate displayed.
2. **Given** a completed trial, **When** the user asks, **Then** the interface verifies that
   evidence without requiring any external tool.
3. **Given** evidence that fails to check out, **When** the interface detects this, **Then** the
   failure is made visible to the user rather than silently ignored.
4. **Given** any visitor, **When** they request the public record, **Then** the complete trial
   history is available for download along with its current head value.
5. **Given** the public record, **When** it is examined, **Then** abandoned trials are present
   and distinguishable from completed ones, and the overall abandonment rate can be computed.
6. **Given** the leaderboard, **When** it is ranked, **Then** an account with very few trials
   cannot occupy a top position on the strength of a short lucky run.
7. **Given** a leaderboard entry, **When** it is displayed, **Then** it shows the chosen name
   together with a distinct public identifier, so that two users choosing the same name remain
   distinguishable.

---

### User Story 4 - Use the site in my own language (Priority: P4)

A German-speaking visitor arrives at the international domain, switches to German, and continues
with their account and history intact.

**Why this priority**: Reaching a wider audience matters only once there is something worth
reaching it with. This is launch preparation, not product substance.

**Independent Test**: Start on one domain, accumulate trials, switch language, and confirm the
account and full history carry over.

**Acceptance Scenarios**:

1. **Given** a visitor on either domain, **When** they use the language switch, **Then** they
   arrive on the other domain in the other language.
2. **Given** a visitor with an existing account, **When** they switch language, **Then** their
   identity and complete trial history carry over without creating a second account.
3. **Given** a visitor switching language while their screen is being shared, **When** the switch
   occurs, **Then** their secret access link is never displayed.
4. **Given** a first-time visitor whose browser prefers another language, **When** they arrive,
   **Then** they are not redirected automatically.
5. **Given** either domain, **When** a visitor looks for legal information, **Then** a data
   protection notice is present in that domain's language.
6. **Given** the moment a name is entered, **When** the account is created, **Then** the visitor
   is told that the chosen name and the complete trial history will be public.
7. **Given** a user holding their access link, **When** they choose to remove their name,
   **Then** it is removed without contacting the operator, and their trials remain in the public
   record under an identifier that no longer names them.

---

### Edge Cases

- **A trial is revealed but never answered** — the visitor sees the eight images and leaves. The
  trial is marked abandoned. It does not count toward the hit rate, the completed-trial total, or
  the ten-trial threshold, but it stays in the public record, distinguishable from completed
  trials, and the abandonment rate is published. Selective abandonment therefore becomes
  measurable rather than being prevented: a user who drops trials that feel wrong is still
  selecting their own sample, and the published counts are what makes that visible.
- **A user wants their name removed** — removal is self-service, proved by holding the access
  link. There is no email on file, so no other route could authenticate the request. The trials
  stay in the record under the account's opaque identifier.
- **Two users choose the same name** — both keep it; the public identifier distinguishes them,
  though a stranger cannot tell which came first.
- **A user loses their access link** — the account and its history become unreachable,
  permanently, with no recovery path.
- **A user reaches ten trials with zero hits** — statistics are shown regardless.
- **Fewer than 200 accounts are eligible** — no ranks are awarded to anyone, and the current
  eligible count is shown in place of the ladder. A ladder whose tiers run to position 200
  cannot be filled by a dozen people without making the top of it meaningless.
- **An account is ranked, then overtaken while inactive** — positional ranks move when others
  play, so a rank already shared elsewhere can go stale. The artefact carries the figures it was
  earned with, so it stays truthful about the moment it describes.
- **A trial is answered faster than the minimum viewing time** — refused without evaluation, the
  trial stays open, and the user may answer again.
- **The image collection is extended between trials** — earlier trials remain verifiable against
  the collection version they were run under.
- **A user opens the same account in two browsers at once** — trials from both must land in one
  history without corrupting the sequence.
- **A user attempts to see the target before choosing** — inspecting network traffic or page
  state must not reveal it.

## Requirements *(mandatory)*

### Functional Requirements

**Access and identity**

- **FR-001**: System MUST allow a visitor to begin playing after entering only a self-chosen
  name, with no registration, email address, or password.
- **FR-002**: System MUST issue each new account a secret personal access link that restores
  that account in any browser.
- **FR-003**: System MUST display the access link masked by default, reveal it only on explicit
  user action, and allow it to be copied without being revealed.
- **FR-004**: System MUST keep the access link permanently reachable but unobtrusive, and MUST
  prompt the user to save it again when they reach the trial count at which statistics unlock.
- **FR-005**: System MUST state plainly that a lost access link cannot be recovered.
- **FR-006**: System MUST NOT expose the access link in any address that is transmitted to or
  recorded by the server.

**The trial**

- **FR-007**: Each trial MUST present a coordinate first and reveal candidate images only after
  a deliberate user action.
- **FR-008**: Each trial MUST present exactly eight candidate images, exactly one of which is
  the target.
- **FR-009**: System MUST fix the target before the user's choice is made and MUST NOT disclose
  it until the choice has been submitted.
- **FR-010**: System MUST report the outcome immediately after the choice and allow the next
  trial to begin in a single action.
- **FR-011**: Candidate images MUST be indistinguishable from one another by any property other
  than their depicted content.
- **FR-012**: Images MUST be drawn from a published, versioned collection of at least 500 freely
  licensed images with recorded provenance, each assigned to one of 16 to 24 subject categories.
- **FR-046**: The eight candidates in a trial MUST come from eight different categories, and the
  target MUST be equally likely to be any of the eight shown, regardless of how many images each
  category holds.
- **FR-013**: System MUST record every trial at the moment it is created, before its outcome is
  known.
- **FR-014**: System MUST treat any trial that has not been completed as abandoned — no elapsed
  time is involved — and MUST retain it rather than deleting it.
- **FR-037**: System MUST accept at most one *evaluated* answer per trial and MUST refuse any
  later answer for a trial already answered. An answer rejected without being evaluated does not
  consume the trial.
- **FR-038**: A trial MUST become permanently uncompletable once its validity period has passed,
  and a user answering after that point MUST be told the trial expired and offered a new one,
  never silently scored as a miss.
- **FR-039**: System MUST reject an answer submitted less than three seconds after the images
  were revealed, and MUST reject it **without evaluating the chosen image**, so that the rule
  cannot be used to probe for the target.

**Scoring and statistics**

- **FR-015**: System MUST track each account's completed trials and hits.
- **FR-016**: Abandoned trials MUST NOT count toward an account's hit rate, its completed-trial
  total, or the threshold at which statistics unlock.
- **FR-017**: System MUST withhold the statistics view until an account has completed ten trials,
  applying that threshold to completed-trial count alone and never to success.
- **FR-018**: Statistics MUST report how far the account's hit rate departs from chance, together
  with how many users would reach that departure by chance alone.
- **FR-019**: Reported statistics MUST advance in blocks of completed trials rather than after
  each individual trial.
- **FR-020**: System MUST publish an aggregate result computed across every trial by every
  account, including accounts that never reached the statistics threshold.
- **FR-021**: System MUST display an account's abandoned-trial count alongside its statistics.

**Verification and the public record**

- **FR-022**: System MUST present, after each revealed outcome, evidence the user can check that
  the target was fixed beforehand and corresponds to the coordinate shown.
- **FR-023**: System MUST verify that evidence within the interface, without any external tool.
- **FR-024**: System MUST make a failed verification visible to the user rather than ignoring it.
- **FR-025**: System MUST offer a public download of the complete trial record together with its
  current head value.
- **FR-026**: The published record MUST NOT contain self-chosen names. Names MUST be held
  separately, so that removing one leaves every published trial intact and verifiable.
- **FR-027**: Abandoned trials MUST appear in the public record, distinguishable from completed
  ones, and the aggregate abandonment rate MUST be published.
- **FR-028**: Leaderboard ranking MUST NOT favour accounts with very few trials.
- **FR-029**: Each leaderboard entry MUST show the chosen name alongside a distinct public
  identifier.
- **FR-040**: An account MUST become leaderboard-eligible only after completing 100 trials
  spread across at least three distinct days.
- **FR-050**: The statistics-unlock count, the leaderboard eligibility floor and the rank band
  edges MUST be server-side configuration, reported in the responses that depend on them and
  stated on the pages that display them, and the interface MUST say that they are adjusted as the
  site grows (D26).
- **FR-041**: The leaderboard MUST display, for every entry, the statistic it is sorted by as
  the primary figure, together with the account's completed-trial count, hit rate and deviation
  from chance.
- **FR-042**: System MUST assign ranks as shares of the eligible population rather than by
  absolute thresholds or fixed seat counts, and MUST award a band only once that band holds at
  least one account without rounding — `share x eligible >= 1` — stating how many accounts are
  currently eligible whether or not any band is active (D23).
- **FR-043**: System MUST assign a distinct rank to accounts performing markedly below chance,
  and MUST display the count of markedly-below and markedly-above accounts together, so the
  symmetry of the two tails is visible.
- **FR-044**: Any shareable artefact depicting an account's rank MUST carry, within the artefact
  itself, that account's completed-trial count and how often such a result occurs by chance.
- **FR-045**: System MUST present the aggregate result as a headline finding, including — and
  without qualification or de-emphasis — when it is indistinguishable from chance.

**Languages and legal**

- **FR-030**: System MUST offer the same functionality in German and English, one language per
  domain.
- **FR-031**: A language switch MUST carry the user's session to the other domain without ever
  placing their secret access link where it can be read from the screen or the address bar.
- **FR-032**: System MUST NOT redirect visitors automatically based on browser language.
- **FR-033**: System MUST present a data protection notice on both domains in that domain's
  language.
- **FR-034**: System MUST disclose, at the moment a name is entered, that the chosen name and
  the complete trial history are public.
- **FR-035**: System MUST allow a user holding a valid access link to remove their chosen name
  themselves, without contacting the operator. Removal is permanent for that account: the account
  remains playable under its opaque identifier and no new name may be set (D25).
- **FR-047**: System MUST NOT display a chosen name on any public surface until it has been
  approved, showing the opaque identifier in its place until then, and MUST tell the account
  holder that their name is under review (D25).
- **FR-048**: System MUST allow a user holding a valid access link to change their name, subject
  to a rate limit. A name refused during review MUST NOT consume that limit, and the previously
  approved name MUST remain displayed until a replacement is approved (D25).
- **FR-049**: Display names need NOT be unique. The public identifier is what distinguishes
  accounts, and MUST therefore be displayed wherever a name is (see FR-029).
- **FR-036**: After a name is removed, the account's trials MUST remain in the public record
  under its opaque identifier.

### Key Entities

- **Account**: an opaque identifier, a secret access credential, and a separately held
  self-chosen display name. The identifier is what appears in the permanent record; the name is
  removable.
- **Trial**: a coordinate, the evidence fixing its outcome in advance, its candidate set, the
  target, the user's choice or its absence, the result, its position in the sequence, and when it
  was created.
- **Image Collection Version**: an ordered, published set of images with a version identity that
  trials refer to, so later additions do not invalidate earlier trials.
- **Image**: depicted content, source, and licence.
- **Public Record**: the append-only sequence of all trials, completed and abandoned, with a head
  value summarising it.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A first-time visitor completes their first trial within 30 seconds of arriving,
  with no registration step.
- **SC-002**: 100% of completed trials publish evidence and a documented procedure sufficient for
  an independent party to verify them without trusting the operator. Shipping a verifier is not
  part of the first release: the in-browser check (SC-003) is written against the same normative
  specification but is served by the operator, so the interface must not claim third-party
  independence it does not have.
- **SC-003**: A user can check a trial's evidence entirely within the interface, needing no
  external tool and no technical knowledge.
- **SC-004**: The published aggregate result is reproducible by a third party from the public
  record alone.
- **SC-005**: Across a large volume of trials by users without ability, the reported aggregate
  hit rate stays within expected sampling bounds of 12.5% — the system demonstrably does not
  inflate results.
- **SC-006**: No account's displayed statistics depend on whether it has recorded any hit.
- **SC-007**: 100% of users switching language retain their account and complete history; no
  duplicate account is created by switching.
- **SC-008**: After a name is removed, 100% of that account's previously published trials remain
  verifiable.
- **SC-009**: No account appears anywhere on the leaderboard without meeting the eligibility floor
  in force, so no short lucky run can occupy a ranked position.
- **SC-010**: The image collection contains at least 160 images at launch, every one freely
  licensed with its provenance recorded, and grows afterwards without invalidating earlier trials
  (D5).
- **SC-011**: Inspecting client-side state or network traffic before a choice is submitted does
  not reveal the target in any trial.
- **SC-017**: Over a large simulated run, the target falls on each of the eight displayed
  positions equally often and shows no relationship to category size — a strategy of always
  choosing the image from the largest category performs no better than 12.5%.
- **SC-012**: The abandonment rate, overall and per account, is computable by a third party from
  the public record alone, so selective abandonment is detectable rather than hidden.
- **SC-013**: No account holds a rank without having met the eligibility rule, and no band is
  awarded while the eligible population is too small for that band to hold one account without
  rounding.
- **SC-018**: No name reaches a public surface without having been approved, and a name refused
  during review is discarded rather than stored.
- **SC-014**: The counts of markedly-above-chance and markedly-below-chance accounts are both
  displayed, so a reader can see whether the two tails are balanced without computing anything.
- **SC-015**: A rank image shared outside the site states the trial count and the by-chance
  frequency it was earned with, so it cannot be read as a claim the site does not make.
- **SC-016**: Answers submitted faster than the minimum viewing time are refused without any
  information about the target being returned.

## Assumptions

- The coordinate is an arbitrary fixed-format reference carrying no information the user could
  decode. It exists because remote viewing convention expects one; it does not encode the target.
- The chance hit rate is 12.5%, one image in eight.
- Candidates are drawn one per category from eight distinct categories, so a trial never shows two
  images of the same kind. Without this, at realistic curation granularity, roughly two trials in
  three would contain a confusable pair.
- "Deviation from chance" is the term used throughout these artifacts for the standardised
  distance between an observed hit rate and 12.5%. It is the same quantity called a z-score
  elsewhere; the interface and the API field use the plainer name.
- A trial is abandoned simply by not being completed — no timer classifies it. Separately, a
  trial stays completable for a validity period assumed to be 24 hours, after which abandonment
  becomes final. Without that bound, a trial in progress and an abandoned one would remain
  indistinguishable forever and the published abandonment rate would never settle. The period is
  tied to how often the trial-token encryption key is rotated, which is what should determine
  the exact figure.
- FR numbers are stable identifiers, not an ordering. FR-037 was added after the first pass and
  sits with the trial requirements rather than at the end, so that existing references keep
  pointing at the same requirements.
- Play itself is not rate-limited over time, but two limits apply: account creation is capped per
  client address, and each account may hold only a small number of uncompleted trials at once. The
  second is what bounds growth of the permanent log, since every trial is recorded at creation.
  Abuse of the leaderboard through many throwaway accounts is **not** answered by a scoring rule,
  because none exists: at true chance there is no signal to rank, so any ordering orders luck and
  more accounts buy more luck (D27). The eligibility rule in FR-040 raises its price, the
  aggregate figure in FR-020 carries the main claim rather than the top entry, and the position is
  stated on the statistics page rather than defended.
- The leaderboard's effective minimum is 100 completed trials, spread over at least three days.
  It is deliberately low at launch, when attracting users matters more than a farm-proof board,
  and it is configuration rather than a constant so it can rise with activity (D26). It is not a
  defence against multi-accounting and is not intended as one (D27).
- The leaderboard is sorted by the lower bound of a confidence interval on the hit rate, which
  weighs trial count rather than surprise alone. A lucky 100-trial run at 25% produces a larger
  deviation from chance than a steady 1000 trials at 15%, and the latter is treated as the
  stronger result.
- The rank ladder is eleven bands, symmetric around *Normie*: *Annunaki* (best 0,1 %),
  *Insektoider Loosh-Farmer* (0,5 %), *Reptiloidenarchont* (2 %), *Grey Alien* (7 %),
  *Psionisches Asset* (20 %), *Normie* (middle 60 %), then mirrored downward through *Zirbeldrüse
  verkalkt*, *Erdstrahlen-Opfer*, *Orgonit-Enjoyer*, *Psi-Nullleiter* and *Kartoffel*. Shares, not
  seats and not thresholds — a share means the same thing at any population size (D23).
- The product's stated position is that it is a public experiment whose most likely outcome is no
  detectable effect, delivered in an irreverent register. This is a premise, not a prediction the
  system enforces: the statistics are computed the same way whatever they show.
- The minimum viewing time is three seconds. Its purpose is data quality rather than automation
  defence — a trial answered in a fraction of a second is click noise. Parallel automation is not
  meaningfully slowed by it; the three-day spread requirement is what raises that cost.
- The interface describes the record as *published*, not as tamper-proof. External anchoring was
  deliberately deferred (D4), so the record's integrity rests on publication and on copies held
  by third parties.
- German and English are the only languages, and each domain is served by its own process started
  with a hard locale switch, both sharing one database (D24). A third language would break the
  one-language-per-domain arrangement and require revisiting it.
- Source availability under a licence obliging modified hosted versions to publish their changes
  is a project-level commitment (D14), not a behaviour of the running system.

## Dependencies

- A curated collection of at least 160 freely licensed images, normalised so that no image is
  distinguishable from another by anything but its content, must exist before User Story 1 is
  playable. This is the largest piece of manual work in the project and it gates the MVP. It also
requires a documented curation workflow that someone other than the author can follow, since the
collection has to keep growing after launch and since diversity across the whole pool — not
per-trial selection — is what keeps a set of eight discriminable.
