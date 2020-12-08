pipeline {
    agent none
    stages {
        stage('Compile Check') {
            agent {
                label "linux && rust"
            }
            steps {
                checkout scm
                sh '''cargo clean'''
                sh '''cargo build'''
            }
        }

        stage("Build Release") {
            parallel {
                stage('Build Release Linux') {
                    agent {
                        label "linux && rust"
                    }

                    steps {
                        // Linux build part
                        checkout scm
                        sh '''cargo clean'''
                        sh '''cargo build --release'''
                        sh '''cp target/release/annepro2_tools target/release/annepro2_tools_linux_x64'''
                    }

                    post {
                        always {
                            archiveArtifacts artifacts: 'target/release/annepro2_tools_linux_x64', caseSensitive: false, fingerprint: true, followSymlinks: false, onlyIfSuccessful: true
                        }
                    }
                }
                stage('Build Release Windows x64') {
                    agent {
                        label "win32 && rust"
                    }

                    steps {
                        // Linux build part
                        checkout scm
                        bat '''cargo clean'''
                        bat '''cargo build --release'''
                        bat '''ren target\\release\\annepro2_tools.exe annepro2_tools_x64.exe'''
                    }

                    post {
                        always {
                            archiveArtifacts artifacts: 'target/release/annepro2_tools_x64.exe', caseSensitive: false, fingerprint: true, followSymlinks: false, onlyIfSuccessful: true
                        }
                    }
                }
                stage('Build Release Windows i386') {
                    agent {
                        label "win32 && rust"
                    }

                    steps {
                        // Linux build part
                        checkout scm
                        bat '''cargo clean'''
                        bat '''cargo build --release --target=i686-pc-windows-msvc'''
                        bat '''ren target\\i686-pc-windows-msvc\\release\\annepro2_tools.exe annepro2_tools_x86.exe'''
                    }

                    post {
                        always {
                            archiveArtifacts artifacts: 'target/i686-pc-windows-msvc/release/annepro2_tools_x86.exe', caseSensitive: false, fingerprint: true, followSymlinks: false, onlyIfSuccessful: true
                        }
                    }
                }
            }
        }
    }
}
