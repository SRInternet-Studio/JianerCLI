# JianerCLI
- 这是一个能够管理本地的JianerCore&JianerQQbot实例的CLI/TUI项目，使用rust开发
- 能够帮助用户快速的创建自己的JianerQQbot实例或者基于JianerCore创建自己的JianerCore项目

***

## **具体实现方案与项目细节**
- 1.终端内输入 ```jianer-cli create``` 可以创建一个 ```JianerCore项目/JianerQQbot项目```，具体如下面树状图所示

```text
创建 JianerCore项目

创建 基于JianerCore的Bot项目
├─ 从 智慧市场 获取Bot项目列表
│  └─ Jianer_QQ_bot
│  └─ Bot2
│  └─ ......
└─ 手动导入Bot项目


```

- 2.当JianerCLI检测到当前的 ```JianerCore项目``` 是 ```Jianer_QQ_bot``` 时，可以开启下面的特殊功能
    - 1.当输入 ```jianer-cli config``` 时，能够便捷的配置Jianer_QQ_bot的配置文件
    - 2.关于 ```jianer-cli plugin```

        ```
        jianer-cli plugin search <search_content>
        ```
        在 智慧市场 中搜索Jianer_QQ_bot插件

        ```
        jianer-cli plugin show --r(emote) --l(ocal) <pluginId>
        ```
        查询某个 Jianer_QQ_bot插件的详细信息，--r(emote)表示远端（智慧市场），--l(ocal)代表本地，不加参数的情况下默认为查询本地插件，如果本地没有安装这个插件则会显示远端的插件信息，远端的插件信息会返回下载量等参数，请一并显示，若本地插件与远端插件版本不同会显示有新的版本可以更新，```<pluginId>```支持模糊匹配
        
        ```
        jianer-cli plugin remove --no-deps <pluginId>
        ```
        删除插件的命令，默认删除依赖，--no-deps代表不删除依赖

        ```
        jianer-cli plugin install -U --all <pluginId>
        ```
        安装插件的命令，-U代表升级安装，不加参数的情况下默认不会去更新本地已有的插件，--all代表全部重新安装至最新版本，无论是否已经安装，并且同时会自动安装插件声明的依赖
    - 3.当输入 ```jianer-cli upgrade```时，从```jianer_qq_bot```的仓库自动检测当前用户项目的更新配置，并从对应渠道拉取更新
    - 4.当输入 ```jianer-cli update-conf```时，从JianerQQbot的仓库自动检测当前已有的代码分支和Release，并让用户选择，同时也可以配置其他的有关于更新的配置
    目前这个仓库的活动分支如下
        - main分支
            - 这是Jianer的主要分支，采用Fixed Release的发行方式（在展示给用户配置的时候显示为```main (Fixed Release)```，使用该分支时默认下载release里的latest，注意标注）
        - NEXT-PREVIEW分支
            - 这是Jianer的未来测试分支，相当于一个Pre-Release分支，采用Curated Rolling Release的发行方式（在展示给用户配置的时候显示为```NEXT-PREVIEW (Curated Rolling Release)```）
        - dev分支
            - 这是Jianer的开发分支，更新频率最高，也最不稳定，不建议投入生产环境使用，采用Rolling Release的更新方式，在展示给用户配置的时候显示为```dev (Rolling Release)```，在选择之后需要强迫用户看10秒免责声明之后才能继续）
        其余分支不用理会


## 关于 智慧市场

智慧市场 是 SR思锐 提供的在线服务，目前尚未完善
