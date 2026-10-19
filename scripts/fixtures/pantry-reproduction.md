REPRODUCTION:

```
{"case":"share-mixed-case","expected":"Weekend shared with Ann","observed":"No user with that email (HTTP 404)","pass":false}
{"case":"ann-sees-weekend","expected":"Weekend under Shared with me","observed":"Nothing shared with you yet.","pass":false}
{"case":"share-lower-case","expected":"Weekend shared with Ann","observed":"Shared with ann@example.com.","pass":true}
```

Signed in as Ben, sharing the Weekend list with `Ann@Example.com` answers "No user with that email" and shares nothing, so Ann's page still says "Nothing shared with you yet." The same address typed in lower case shares the list: the share looks the address up exactly as it was typed, and every stored email is lower case. That is the "sometimes" in the report.

The app at `main` ({{commit}}), {{now}}. Harness: `.repro/{{key}}/repro.mjs`; case screenshots attached.
