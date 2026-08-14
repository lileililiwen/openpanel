# Add Per-site collaborator permissions — Design

## SiteGrant model

```rust
pub struct SiteGrant {
    pub collaborator_id: CollaboratorId,
    pub site_id: SiteId,
    pub permissions: PermissionSet, // bits: File | Database | Mail | Cron
}
```

## Collaborator

```rust
pub struct Collaborator {
    pub collaborator_id: CollaboratorId,
    pub account_id: AccountId,     // belongs to the inviting account
    pub email: Email,
    pub status: CollabStatus,      // Invited | Active | Revoked
}
```

## Invite / resolve flow

```
invite(account, site_id, email, permissions):
  create Collaborator{status=Invited} under account
  create SiteGrant{site_id, permissions}
  send invite; audit CollaboratorInvited

resolve_access(principal, site_id):
  grants = SiteGrant::for(principal, site_id)
  effective = union(grants.permissions)   // NOT account role
  return effective   // used by adapters to gate file/db/mail/cron

revoke(collaborator_id, site_id):
  delete SiteGrant; optionally set status=Revoked
  audit CollaboratorRevoked
```

## Endpoints

```
POST   /api/v1/sites/{id}/collaborators        body { email, permissions }
GET    /api/v1/sites/{id}/collaborators/{uid}
PUT    /api/v1/sites/{id}/collaborators/{uid}  body { permissions? }
DELETE /api/v1/sites/{id}/collaborators/{uid}
```

## Tests

```
1.1 Unit: permission union across grants; scope bit masking.
1.2 Property: collaborator access never equals account role; revoke
    removes all access for the site.
1.3 Service tests w/ mock identity: invite, resolve, revoke.
1.4 Integration: collaborator can touch granted scope only; revoke
    blocks access.
1.5 Web: Collaborators tab (CSRF), invite form, scope toggles.
```
