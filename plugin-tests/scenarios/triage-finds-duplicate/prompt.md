A customer just reported this on board ACME: if you submit the sign-in form
without typing a password, the server blows up with an internal server error
instead of telling you the field is required. Their log line is
`NullPointerException at AuthService.verify`. Can you get this triaged?
