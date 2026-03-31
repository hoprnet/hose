{{/*
Expand the name of the chart.
*/}}
{{- define "hose.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create a default fully qualified app name.
Truncated at 63 characters because some Kubernetes name fields are limited to this.
If fullnameOverride is provided, use that. Otherwise, if the release name contains
the chart name, use the release name; otherwise combine them.
*/}}
{{- define "hose.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- if contains $name .Release.Name }}
{{- .Release.Name | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}
{{- end }}

{{/*
Create chart name and version as used by the chart label.
*/}}
{{- define "hose.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels applied to all resources.
*/}}
{{- define "hose.labels" -}}
helm.sh/chart: {{ include "hose.chart" . }}
{{ include "hose.selectorLabels" . }}
{{- if .Chart.AppVersion }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
{{- end }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end }}

{{/*
Selector labels used for matching pods to services and deployments.
*/}}
{{- define "hose.selectorLabels" -}}
app.kubernetes.io/name: {{ include "hose.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{/*
Create the name of the service account to use.
*/}}
{{- define "hose.serviceAccountName" -}}
{{- if .Values.serviceAccount.create }}
{{- default (include "hose.fullname" .) .Values.serviceAccount.name }}
{{- else }}
{{- default "default" .Values.serviceAccount.name }}
{{- end }}
{{- end }}

{{/*
Extract the port number from a "host:port" listen address string.
Example: {{ include "hose.port" "0.0.0.0:8080" }} => 8080
*/}}
{{- define "hose.port" -}}
{{- $parts := splitList ":" . -}}
{{- last $parts -}}
{{- end -}}
