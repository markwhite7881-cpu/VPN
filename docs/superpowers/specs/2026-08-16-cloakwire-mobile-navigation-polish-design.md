# Cloakwire mobile navigation polish

## Goal

Move diagnostic logs out of the persistent bottom navigation and make mobile tab transitions feel intentional without weakening the existing guarded swipe behavior.

## Scope

- Bottom navigation contains Home, Servers, Routing, and Settings only.
- Settings presents a compact Logs row that opens the existing Logs screen.
- Logs provides an explicit route back to Settings.
- Tab changes use a short directional slide-and-fade animation. A swipe uses its gesture direction; a bottom-navigation tap uses relative tab order.
- Users who request reduced motion receive an immediate transition.

## Constraints

- Keep existing Logs data loading and rendering behavior; do not introduce new bridge APIs.
- Preserve all protected-swipe exclusions for controls, editable fields, scrollable containers, text selection, and vertical gestures.
- Preserve shadcn semantic styling tokens and avoid dependencies.
- Do not change VPN configuration, reconnect behavior, routing logic, or persistence formats.

## Verification

- Extend the deterministic UI check for the four persistent tabs, Settings → Logs routing, and animation/reduced-motion classes.
- Run `node scripts/check-mobile-ui.mjs`, `npm run build`, and `git diff --check`.
- Build and install the Android debug APK; manually verify bottom navigation, Settings → Logs → Settings, swipe direction animation, tap animation, and that guarded gestures still do not switch views.
