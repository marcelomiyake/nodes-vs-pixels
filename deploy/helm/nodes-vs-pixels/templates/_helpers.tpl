{{/* Labels shared by every object of a component. */}}
{{- define "nodes-vs-pixels.labels" -}}
app.kubernetes.io/name: {{ .component }}
app.kubernetes.io/instance: {{ .root.Release.Name }}
app.kubernetes.io/part-of: {{ .root.Chart.Name }}
app.kubernetes.io/version: {{ .root.Chart.AppVersion | quote }}
helm.sh/chart: {{ printf "%s-%s" .root.Chart.Name .root.Chart.Version }}
{{- end }}

{{/* Labels that select a component's pods. */}}
{{- define "nodes-vs-pixels.selector" -}}
app.kubernetes.io/name: {{ .component }}
app.kubernetes.io/instance: {{ .root.Release.Name }}
{{- end }}

{{/*
Locks a container down: runs as the numeric user given (Kubernetes can only
verify a numeric user is not root), without privilege escalation or capabilities.
*/}}
{{- define "nodes-vs-pixels.containerSecurity" -}}
allowPrivilegeEscalation: false
readOnlyRootFilesystem: true
runAsNonRoot: true
runAsUser: {{ . }}
runAsGroup: {{ . }}
capabilities:
  drop: ["ALL"]
seccompProfile:
  type: RuntimeDefault
{{- end }}
