# AI Gateway Payload Processing

This repository contains Payload Processing plugins that will be connected to an AI Gateway via pluggable BBR (Body Based Routing) framework that was developed as part of [Kuberenetes Inference Gateway](https://github.com/kubernetes-sigs/gateway-api-inference-extension).

BBR plugins enable custom request/reseponse mutations of both headers and body, allowing advanced capabilites such as promoting the model from a field in the body to a header and route to a selected endpoint accordingly.
