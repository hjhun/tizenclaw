Name:       python3-protobuf
Summary:    Python 3 bindings for Protocol Buffers
Version:    5.29.3
Release:    1
Group:      Development/Libraries
License:    BSD-3-Clause
Source0:    protobuf-%{version}.tar.gz
BuildRequires:  python3
BuildRequires:  python3-devel
BuildRequires:  gcc-c++

%description
Python 3 bindings for Protocol Buffers. This package is built for Tizen armv7l.

%prep
%setup -q -n protobuf-%{version}

%build
# Go to the python directory inside the protobuf source
cd python
python3 setup.py build --cpp_implementation

%install
rm -rf %{buildroot}
cd python
python3 setup.py install --root=%{buildroot} --prefix=/usr

%clean
rm -rf %{buildroot}

%files
%defattr(-,root,root,-)
/usr/lib*/python3*/site-packages/*
