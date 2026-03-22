Name:       python3-onnxruntime
Summary:    Python 3 bindings for ONNX Runtime
Version:    1.20.1
Release:    1
Group:      Development/Libraries
License:    MIT
Source0:    onnxruntime-%{version}.tar.gz
BuildRequires:  python3
BuildRequires:  python3-devel
BuildRequires:  python3-numpy
BuildRequires:  cmake
BuildRequires:  gcc-c++

%description
ONNX Runtime Python bindings for Tizen armv7l.

%prep
%setup -q -n onnxruntime-%{version}

%build
# Cross compilation cmake arguments for ONNX Runtime
./build.sh --config Release --build_shared_lib --build_wheel --update --build

%install
rm -rf %{buildroot}
# Install the built wheel
pip install build/Linux/Release/dist/*.whl --target %{buildroot}/usr/lib/python3.14/site-packages/

%clean
rm -rf %{buildroot}

%files
%defattr(-,root,root,-)
/usr/lib*/python3*/site-packages/onnxruntime/
/usr/lib*/python3*/site-packages/onnxruntime*.dist-info/
