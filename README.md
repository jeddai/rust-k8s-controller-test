# Rust k8s controller test

Hi I decided to make a k8s controller in Rust. Just trying to learn the ControlPlane API and stuff!

### Usage

```bash
# Apply the role-based access control and controller deployment files
kubectl apply -f example-configurations/rbac.yml -f example-configurations/controller.yml
# Apply the sample deployment that will be picked up by the controller
kubectl apply -f example-configurations/test-deployment.yml
```